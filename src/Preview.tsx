import { useState, useEffect, useRef } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";
import { LogicalSize } from "@tauri-apps/api/dpi";
import html2canvas from "html2canvas";
import { Checkbox } from "./components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./components/ui/select";

const SHARE_TEMPLATES = [
  { id: "clean", label: "简约" },
  { id: "card", label: "卡片" },
  { id: "polaroid", label: "拍立得" },
  { id: "quote", label: "引用" },
] as const;

type TemplateId = (typeof SHARE_TEMPLATES)[number]["id"];

export default function Preview() {
  const params = new URLSearchParams(window.location.search);
  const path = params.get("path") || "";
  const mode = params.get("mode") || "preview";
  const isCaptionMode = mode === "caption";

  const [caption, setCaption] = useState("");
  const [template, setTemplate] = useState<TemplateId>("clean");
  const [composing, setComposing] = useState(false);
  const [imageLoaded, setImageLoaded] = useState(false);
  const [showWatermark, setShowWatermark] = useState(true);
  const [watermarkPosition, setWatermarkPosition] = useState("bottom_right");
  const [screenshotCount, setScreenshotCount] = useState(0);

  // Load watermark settings from config
  useEffect(() => {
    console.log('[DEBUG][Preview] Loading watermark settings...');
    invoke<string>("get_watermark_position").then((pos) => {
      console.log('[DEBUG][Preview] Loaded watermark position:', pos);
      setWatermarkPosition(pos);
      setShowWatermark(pos !== "none");
    }).catch(console.error);
    invoke<number>("get_screenshot_count").then((count) => {
      console.log('[DEBUG][Preview] Screenshot count:', count);
      setScreenshotCount(count);
    }).catch(console.error);
  }, []);
  const captionRef = useRef(caption);
  const renderRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  captionRef.current = caption;

  const [imgSize, setImgSize] = useState({ width: 0, height: 0 });

  const resizeWindowToFit = async (naturalWidth: number, naturalHeight: number) => {
    const windowWidth = 480;
    const contentWidth = windowWidth - 16 - 32; // margin + padding
    const scale = Math.min(1, contentWidth / naturalWidth);
    const scaledHeight = naturalHeight * scale;
    // Add padding for template chrome (varies by template)
    const templatePadding = template === "polaroid" ? 120 : template === "card" ? 80 : 60;
    const previewHeight = scaledHeight + templatePadding;
    const controlsHeight = 160;
    const windowPadding = 32;
    const totalHeight = previewHeight + controlsHeight + windowPadding;
    const height = Math.min(Math.max(totalHeight, 320), 800);
    try {
      await getCurrentWindow().setSize(new LogicalSize(windowWidth, height));
    } catch (e) {
      console.error("[Preview] resize failed:", e);
    }
  };

  const handleClose = async () => {
    try {
      await getCurrentWindow().close();
    } catch (e) {
      console.error("[Preview] close failed:", e);
    }
  };

  const handleClick = async () => {
    if (!isCaptionMode) {
      await handleClose();
    }
  };

  // Load existing description when opening caption editor
  useEffect(() => {
    if (!isCaptionMode || !path) return;
    invoke<string | null>("get_image_description", { path }).then((desc) => {
      if (desc) setCaption(desc);
    }).catch(console.error);
  }, [isCaptionMode, path]);

  // Auto-save and close on window blur
  useEffect(() => {
    if (!isCaptionMode) return;

    const win = getCurrentWindow();
    const unlisten = win.onFocusChanged(async ({ payload: focused }) => {
      if (!focused) {
        if (captionRef.current.trim()) {
          await invoke("save_caption", { path, caption: captionRef.current.trim() });
        }
        await win.close();
      }
    });

    return () => { unlisten.then((f) => f()); };
  }, [isCaptionMode, path]);

  useEffect(() => {
    if (!isCaptionMode) return;

    const onKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        // Save caption before closing if modified
        if (captionRef.current.trim()) {
          await invoke("save_caption", { path, caption: captionRef.current.trim() });
        }
        await getCurrentWindow().close();
      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleCopyComposed();
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isCaptionMode, path]);

  // Resize window when template changes
  useEffect(() => {
    if (isCaptionMode && imageLoaded && imgSize.width > 0) {
      resizeWindowToFit(imgSize.width, imgSize.height);
    }
  }, [template, isCaptionMode, imageLoaded, imgSize]);

  const handleCopyComposed = async () => {
    if (composing || !renderRef.current || !captionRef.current.trim()) return;
    setComposing(true);

    try {
      const canvas = await html2canvas(renderRef.current, {
        backgroundColor: null,
        scale: 2, // 2x for retina
        useCORS: true,
        logging: false,
      });

      // Get image data and send to Rust for clipboard
      const ctx = canvas.getContext("2d");
      if (!ctx) {
        setComposing(false);
        return;
      }
      const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
      const rgba = Array.from(imageData.data);

      await invoke("copy_rgba_to_clipboard", {
        data: rgba,
        width: canvas.width,
        height: canvas.height,
      });
      // Save caption to original image's Finder comment
      if (captionRef.current.trim()) {
        await invoke("save_caption", { path, caption: captionRef.current.trim() });
      }
      console.log("[Preview] Image copied to clipboard via Tauri");
    } catch (err) {
      console.error("[Preview] Copy failed:", err);
    } finally {
      setComposing(false);
    }
  };

  // Render template content for html2canvas capture
  const renderTemplateContent = () => {
    const imageSrc = convertFileSrc(path);

    const handleImageLoad = (e: React.SyntheticEvent<HTMLImageElement>) => {
      const img = e.currentTarget;
      setImgSize({ width: img.naturalWidth, height: img.naturalHeight });
      setImageLoaded(true);
      resizeWindowToFit(img.naturalWidth, img.naturalHeight);
    };

    // Watermark with optional numbering based on position
    const watermarkText = watermarkPosition === "brand" && screenshotCount > 0
      ? `#${screenshotCount} via lovshot`
      : "via lovshot";
    const watermark = showWatermark && <span className="tpl-watermark">{watermarkText}</span>;

    switch (template) {
      case "clean":
        return (
          <div className="tpl-clean">
            <img src={imageSrc} alt="" onLoad={handleImageLoad} />
            {caption && <p className="tpl-caption">{caption}</p>}
            {watermark}
          </div>
        );

      case "card":
        return (
          <div className="tpl-card">
            <div className="tpl-card-inner">
              <img src={imageSrc} alt="" onLoad={handleImageLoad} />
              {caption && <p className="tpl-caption">{caption}</p>}
              {watermark}
            </div>
          </div>
        );

      case "polaroid":
        return (
          <div className="tpl-polaroid">
            <div className="tpl-polaroid-frame">
              <img src={imageSrc} alt="" onLoad={handleImageLoad} />
              <div className="tpl-polaroid-bottom">
                {caption && <p className="tpl-caption">{caption}</p>}
                {watermark}
              </div>
            </div>
          </div>
        );

      case "quote":
        return (
          <div className="tpl-quote">
            {caption && (
              <blockquote className="tpl-quote-text">"{caption}"</blockquote>
            )}
            <img src={imageSrc} alt="" onLoad={handleImageLoad} />
            <div className="tpl-quote-footer">{showWatermark ? "via lovshot" : ""}</div>
          </div>
        );

      default:
        return null;
    }
  };

  if (isCaptionMode) {
    return (
      <div className="share-editor">
        {/* Hidden render target for html2canvas */}
        <div
          ref={renderRef}
          className={`share-render tpl-${template}`}
          style={{
            position: "absolute",
            left: "-9999px",
            top: 0,
          }}
        >
          {renderTemplateContent()}
        </div>

        {/* Visible preview area */}
        <div className="share-preview" data-tauri-drag-region>
          <div ref={previewRef} className={`share-preview-inner tpl-${template}`}>
            {renderTemplateContent()}
          </div>
        </div>

        {/* Bottom controls */}
        <div className="share-controls">
          <div className="template-tabs">
            {SHARE_TEMPLATES.map((t) => (
              <button
                key={t.id}
                className={`template-tab${template === t.id ? " active" : ""}`}
                onClick={() => setTemplate(t.id)}
              >
                {t.label}
              </button>
            ))}
          </div>
          <textarea
            className="caption-input"
            placeholder="添加说明文字…"
            value={caption}
            onChange={(e) => {
              const val = e.target.value;
              setCaption(val);
              emit("caption-typing", { path, caption: val });
            }}
            autoFocus
          />
          <div className="share-footer">
            <div className="flex items-center gap-4">
              <label className="flex items-center gap-2 text-xs text-muted-foreground cursor-pointer select-none">
                <Checkbox
                  checked={showWatermark}
                  onCheckedChange={(checked) => setShowWatermark(checked === true)}
                />
                <span>水印</span>
              </label>
              <div className="flex items-center gap-2 text-xs text-muted-foreground">
                <span>编号</span>
                <Select
                  value={watermarkPosition}
                  onValueChange={async (pos) => {
                    setWatermarkPosition(pos);
                    if (pos === "none") setShowWatermark(false);
                    await invoke("set_watermark_position", { position: pos });
                  }}
                >
                  <SelectTrigger className="h-7 w-24 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="none">关闭</SelectItem>
                    <SelectItem value="brand">品牌名旁</SelectItem>
                    <SelectItem value="top_left">左上角</SelectItem>
                    <SelectItem value="top_right">右上角</SelectItem>
                    <SelectItem value="bottom_left">左下角</SelectItem>
                    <SelectItem value="bottom_right">右下角</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
            <div className="share-actions">
              <button
                className="share-btn"
                onClick={handleCopyComposed}
                disabled={composing || !caption.trim() || !imageLoaded}
              >
                {composing ? "生成中…" : "复制"}
              </button>
              <button
                className="share-btn secondary"
                onClick={async () => {
                  if (composing || !renderRef.current || !caption.trim()) return;
                  setComposing(true);
                  try {
                    const canvas = await html2canvas(renderRef.current, {
                      backgroundColor: null,
                      scale: 2,
                      useCORS: true,
                      logging: false,
                    });
                    const ctx = canvas.getContext("2d");
                    if (!ctx) return;
                    const imageData = ctx.getImageData(0, 0, canvas.width, canvas.height);
                    const rgba = Array.from(imageData.data);
                    // Generate new filename with timestamp
                    const dir = path.substring(0, path.lastIndexOf("/"));
                    const ext = path.substring(path.lastIndexOf("."));
                    const baseName = path.substring(path.lastIndexOf("/") + 1, path.lastIndexOf("."));
                    const newPath = `${dir}/${baseName}_${Date.now()}${ext}`;
                    await invoke("save_rgba_to_file", {
                      data: rgba,
                      width: canvas.width,
                      height: canvas.height,
                      path: newPath,
                    });
                    // Save caption to original image's Finder comment
                    if (caption.trim()) {
                      await invoke("save_caption", { path, caption: caption.trim() });
                    }
                    await emit("image-saved", { path: newPath });
                    console.log("[Preview] Image saved:", newPath);
                  } catch (err) {
                    console.error("[Preview] Save failed:", err);
                  } finally {
                    setComposing(false);
                  }
                }}
                disabled={composing || !caption.trim() || !imageLoaded}
                title="保存合成图为新文件"
              >
                保存
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="preview-container" onClick={handleClick}>
      {path && <img src={convertFileSrc(path)} alt="Screenshot" />}
      <div className="preview-label">已保存到剪贴板</div>
    </div>
  );
}
