import { useState, useEffect, useRef } from "react";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emit } from "@tauri-apps/api/event";

export default function Preview() {
  const params = new URLSearchParams(window.location.search);
  const path = params.get("path") || "";
  const mode = params.get("mode") || "preview";
  const isCaptionMode = mode === "caption";

  const [caption, setCaption] = useState("");
  const captionRef = useRef(caption);
  captionRef.current = caption;

  const handleClose = async () => {
    console.log("[Preview] handleClose called");
    try {
      await getCurrentWindow().close();
      console.log("[Preview] close completed");
    } catch (e) {
      console.error("[Preview] close failed:", e);
    }
  };

  const handleClick = async () => {
    if (!isCaptionMode) {
      await handleClose();
    }
  };

  const handleSave = async () => {
    console.log("[Preview] handleSave called");
    const text = captionRef.current.trim();
    if (text) {
      invoke("save_caption", { path, caption: text }).catch(console.error);
    }
    await handleClose();
  };

  useEffect(() => {
    if (!isCaptionMode) return;

    const onKeyDown = async (e: KeyboardEvent) => {
      console.log("[Preview] keydown:", e.key);
      if (e.key === "Escape") {
        e.preventDefault();
        console.log("[Preview] ESC pressed, closing...");
        try {
          await getCurrentWindow().close();
        } catch (err) {
          console.error("[Preview] ESC close failed:", err);
        }
      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        console.log("[Preview] Cmd+Enter pressed, saving...");
        const text = captionRef.current.trim();
        if (text) {
          invoke("save_caption", { path, caption: text }).catch(console.error);
        }
        try {
          await getCurrentWindow().close();
        } catch (err) {
          console.error("[Preview] save close failed:", err);
        }
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [isCaptionMode, path]);

  if (isCaptionMode) {
    return (
      <div className="caption-container">
        <div className="caption-image-wrapper" data-tauri-drag-region>
          {path && <img src={convertFileSrc(path)} alt="Screenshot" />}
        </div>
        <div className="caption-input-area">
          <textarea
            className="caption-input"
            placeholder="添加图片描述..."
            value={caption}
            onChange={(e) => {
              const val = e.target.value;
              setCaption(val);
              emit("caption-typing", { path, caption: val });
            }}
            autoFocus
          />
          <div className="caption-shortcuts">
            <kbd>↵</kbd> 换行 · <kbd>⌘</kbd><kbd>↵</kbd> 保存 · <kbd>esc</kbd> 跳过
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
