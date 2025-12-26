import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { save } from "@tauri-apps/plugin-dialog";
import type Konva from "konva";
import { AnnotationCanvas } from "./components/AnnotationCanvas";
import { Magnifier } from "./components/Magnifier";
import { useAnnotationEditor } from "./hooks/useAnnotationEditor";
import type { AnnotationTool } from "./types/annotation";
import { ANNOTATION_COLORS, STYLE_OPTIONS } from "./types/annotation";

type Mode = "image" | "staticimage" | "gif" | "video" | "scroll";
type ResizeDirection = "n" | "s" | "e" | "w" | "ne" | "nw" | "se" | "sw" | null;

interface SelectionRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface WindowInfo {
  x: number;
  y: number;
  width: number;
  height: number;
  titlebar_height: number;
}

export default function Selector() {
  const [isSelecting, setIsSelecting] = useState(false);
  const [selectionRect, setSelectionRect] = useState<SelectionRect | null>(null);
  const [mode, setMode] = useState<Mode>("image");
  const [showHint, setShowHint] = useState(true);
  const [showToolbar, setShowToolbar] = useState(false);
  const [mousePos, setMousePos] = useState<{ x: number; y: number } | null>(null);
  const [hoveredWindow, setHoveredWindow] = useState<SelectionRect | null>(null);
  const [resizeDir, setResizeDir] = useState<ResizeDirection>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [excludeTitlebar, setExcludeTitlebar] = useState(false);
  const [currentTitlebarHeight, setCurrentTitlebarHeight] = useState(0);
  const [originalWindowInfo, setOriginalWindowInfo] = useState<WindowInfo | null>(null);
  const [scrollCaptureEnabled, setScrollCaptureEnabled] = useState(false);
  const [screenSnapshot, setScreenSnapshot] = useState<string | null>(null);
  const [magnifierReady, setMagnifierReady] = useState(false);
  const [captionEnabled, setCaptionEnabled] = useState(() => {
    return localStorage.getItem("captionEnabled") === "true";
  });
  const currentColorRef = useRef<string>("#000000");

  // Annotation editing state
  const [isEditing, setIsEditing] = useState(false);
  const [previewImage, setPreviewImage] = useState<string | null>(null);
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  const [_openDropdown, setOpenDropdown] = useState<string | null>(null);
  const stageRef = useRef<Konva.Stage>(null);
  const editor = useAnnotationEditor();

  // Scroll capture state (事件驱动，不再轮询)
  const [scrollCaptureActive, setScrollCaptureActive] = useState(false);
  const [scrollPreview, setScrollPreview] = useState<string | null>(null);
  const [scrollFrameCount, setScrollFrameCount] = useState(0);
  const [scrollTotalHeight, setScrollTotalHeight] = useState(0);

  // Persist captionEnabled
  useEffect(() => {
    localStorage.setItem("captionEnabled", String(captionEnabled));
  }, [captionEnabled]);

  const startPos = useRef({ x: 0, y: 0 });
  const endPos = useRef({ x: 0, y: 0 });
  const startRect = useRef<SelectionRect | null>(null);
  const selectionRef = useRef<HTMLDivElement>(null);
  const sizeRef = useRef<HTMLDivElement>(null);
  const lastDetectTime = useRef(0);

  const closeWindow = useCallback(async () => {
    await getCurrentWindow().close();
  }, []);

  // Enter annotation editing mode
  const enterEditMode = useCallback(async (tool: AnnotationTool) => {
    if (!selectionRect) return;

    // Capture the region as preview image
    const region = {
      x: Math.round(selectionRect.x),
      y: Math.round(selectionRect.y),
      width: Math.round(selectionRect.w),
      height: Math.round(selectionRect.h),
    };

    try {
      const preview = await invoke<string>("capture_region_preview", { region });
      setPreviewImage(preview);
      setIsEditing(true);
      editor.setActiveTool(tool);
    } catch (e) {
      console.error("[Selector] Failed to capture preview:", e);
    }
  }, [selectionRect, editor]);

  // Exit editing mode
  const exitEditMode = useCallback(() => {
    setIsEditing(false);
    setPreviewImage(null);
    editor.reset();
  }, [editor]);

  // 监听后端的滚动捕获事件（事件驱动，替代轮询）
  useEffect(() => {
    if (!scrollCaptureActive) return;

    let unlisten: (() => void) | null = null;

    listen<{ frame_count: number; total_height: number; preview_base64: string }>(
      "scroll-preview-update",
      (event) => {
        console.log("[Selector] 收到滚动更新事件, frames:", event.payload.frame_count);
        setScrollPreview(event.payload.preview_base64);
        setScrollFrameCount(event.payload.frame_count);
        setScrollTotalHeight(event.payload.total_height);
      }
    ).then((fn) => {
      unlisten = fn;
    });

    return () => {
      if (unlisten) unlisten();
    };
  }, [scrollCaptureActive]);

  // 滚动截图：完成并保存
  const finishScrollCapture = useCallback(async () => {
    if (!scrollCaptureActive) return;

    try {
      // 先禁用穿透，让保存对话框可以正常交互
      await invoke("set_selector_mouse_passthrough", { enabled: false });

      // 弹出保存对话框
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const filePath = await save({
        defaultPath: `scroll_${timestamp}.png`,
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });

      if (!filePath) {
        // 用户取消了保存，重新启用穿透
        await invoke("set_selector_mouse_passthrough", { enabled: true });
        return;
      }

      // 保存并关闭
      setScrollCaptureActive(false);
      await invoke("finish_scroll_capture", { path: filePath, crop: null });
      await closeWindow();
    } catch (e) {
      console.error("[Selector] Failed to finish scroll capture:", e);
    }
  }, [scrollCaptureActive, closeWindow]);

  // 滚动截图：取消
  const cancelScrollCapture = useCallback(async () => {
    if (!scrollCaptureActive) return;

    // Stop polling FIRST to prevent new FFT matching requests
    setScrollCaptureActive(false);
    setScrollPreview(null);
    setScrollFrameCount(0);
    setScrollTotalHeight(0);

    try {
      await invoke("set_selector_mouse_passthrough", { enabled: false });
      await invoke("cancel_scroll_capture");
    } catch {
      // ignore
    }
    await closeWindow();
  }, [scrollCaptureActive, closeWindow]);

  // Fetch pending mode and config on mount
  useEffect(() => {
    invoke<Mode | null>("get_pending_mode").then((pendingMode) => {
      console.log("[Selector] get_pending_mode 返回:", pendingMode);
      if (pendingMode) {
        setMode(pendingMode);
        // For static mode, fetch the pre-captured screenshot
        if (pendingMode === "staticimage") {
          invoke<string | null>("get_screen_snapshot").then((snapshot) => {
            if (snapshot) {
              console.log("[Selector] 获取到静态截图");
              setScreenSnapshot(snapshot);
            }
          });
        }
        invoke("clear_pending_mode");
      }
    });
    // Check if scroll capture is enabled
    invoke<{ scroll_capture_enabled: boolean }>("get_shortcuts_config").then((cfg) => {
      setScrollCaptureEnabled(cfg.scroll_capture_enabled);
    });
    // Check if magnifier is ready (cached_snapshot exists)
    // Use a short poll since RGBA caching is very fast (~30ms)
    let attempts = 0;
    const checkReady = setInterval(() => {
      invoke<number[] | null>("get_magnifier_pixels", { x: 0, y: 0, size: 1 }).then((pixels) => {
        if (pixels) {
          console.log("[Selector] 放大镜就绪");
          setMagnifierReady(true);
          clearInterval(checkReady);
        } else if (++attempts > 20) {
          console.warn("[Selector] 放大镜初始化超时");
          clearInterval(checkReady);
        }
      });
    }, 50);

    return () => clearInterval(checkReady);
  }, []);

  // Track mouse position and detect window under cursor (throttled)
  useEffect(() => {
    const handler = async (e: MouseEvent) => {
      setMousePos({ x: e.clientX, y: e.clientY });

      if (isSelecting || showToolbar) return;

      const now = Date.now();
      if (now - lastDetectTime.current < 50) return;
      lastDetectTime.current = now;

      const windowRegion = await invoke<{ x: number; y: number; width: number; height: number } | null>(
        "get_window_at_cursor"
      );

      if (windowRegion) {
        setHoveredWindow({
          x: windowRegion.x,
          y: windowRegion.y,
          w: windowRegion.width,
          h: windowRegion.height,
        });
      } else {
        setHoveredWindow(null);
      }
    };
    document.addEventListener("mousemove", handler);

    invoke<[number, number] | null>("get_mouse_position").then((pos) => {
      if (pos) setMousePos({ x: pos[0], y: pos[1] });
    });
    invoke<{ x: number; y: number; width: number; height: number } | null>("get_window_at_cursor").then((win) => {
      if (win) setHoveredWindow({ x: win.x, y: win.y, w: win.width, h: win.height });
    });

    return () => document.removeEventListener("mousemove", handler);
  }, [isSelecting, showToolbar]);

  const doCapture = useCallback(async () => {
    console.log("[Selector] doCapture called, mode:", mode, "selectionRect:", selectionRect, "isEditing:", isEditing);
    if (!selectionRect) return;

    const region = {
      x: Math.round(selectionRect.x),
      y: Math.round(selectionRect.y),
      width: Math.round(selectionRect.w),
      height: Math.round(selectionRect.h),
    };

    await invoke("set_region", { region });

    if (mode === "image" || mode === "staticimage") {
      const win = getCurrentWindow();

      // If in editing mode with annotations, export the canvas
      if (isEditing && stageRef.current && editor.annotations.length > 0) {
        try {
          // Hide transformer before export
          const transformer = stageRef.current.findOne("Transformer");
          transformer?.hide();

          // Export canvas to base64
          const dataUrl = stageRef.current.toDataURL({ pixelRatio: window.devicePixelRatio });
          const base64 = dataUrl.replace(/^data:image\/png;base64,/, "");

          transformer?.show();

          await win.hide();
          await invoke("save_annotated_screenshot", { imageData: base64, captionMode: captionEnabled });
          await win.close();
          return;
        } catch (e) {
          console.error("[Selector] Failed to export annotated screenshot:", e);
        }
      }

      // Normal screenshot flow
      await win.hide();
      // Static mode doesn't need delay since we use cached screenshot
      if (mode === "image") {
        await new Promise((r) => setTimeout(r, 50));
      }
      await invoke("save_screenshot", { useCached: mode === "staticimage", captionMode: captionEnabled });
      await win.close();
    } else if (mode === "gif") {
      await invoke("start_recording");
      await closeWindow();
    } else if (mode === "scroll") {
      // Scroll mode: 内联滚动捕获（统一窗口方案）
      console.log("[Selector] 进入 scroll 模式（内联）");

      try {
        // 启用鼠标穿透，让滚动事件传递到下层窗口
        await invoke("set_selector_mouse_passthrough", { enabled: true });
        // 启动滚动捕获
        const progress = await invoke<{ frame_count: number; total_height: number; preview_base64: string }>(
          "start_scroll_capture_inline",
          { region }
        );
        setScrollPreview(progress.preview_base64);
        setScrollFrameCount(progress.frame_count);
        setScrollTotalHeight(progress.total_height);
        setScrollCaptureActive(true);
      } catch (e) {
        console.error("[Selector] Failed to start scroll capture:", e);
        await invoke("set_selector_mouse_passthrough", { enabled: false });
      }
    }
  }, [selectionRect, mode, closeWindow, isEditing, editor.annotations.length, captionEnabled]);

  // Resize handle start
  const handleResizeStart = useCallback(
    (dir: ResizeDirection) => (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!selectionRect) return;
      setResizeDir(dir);
      startPos.current = { x: e.clientX, y: e.clientY };
      startRect.current = { ...selectionRect };
    },
    [selectionRect]
  );

  // Drag selection start
  const handleDragStart = useCallback(
    (e: React.MouseEvent) => {
      e.stopPropagation();
      if (!selectionRect) return;
      setIsDragging(true);
      startPos.current = { x: e.clientX, y: e.clientY };
      startRect.current = { ...selectionRect };
    },
    [selectionRect]
  );

  // Mouse events
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("#toolbar")) return;
    if ((e.target as HTMLElement).closest(".resize-handle")) return;
    if ((e.target as HTMLElement).closest(".annotation-canvas")) return;
    if ((e.target as HTMLElement).closest(".tool-dropdown")) return;

    // Close dropdown when clicking outside
    setOpenDropdown(null);

    // In editing mode, don't reset selection
    if (isEditing) return;

    setShowToolbar(false);
    setSelectionRect(null);
    setShowHint(false);
    // 不立即清除 hoveredWindow，让窗口高亮在拖拽时保持显示作为参考

    startPos.current = { x: e.clientX, y: e.clientY };
    setIsSelecting(true);
  }, [isEditing]);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      // Handle drag move
      if (isDragging && startRect.current) {
        const dx = e.clientX - startPos.current.x;
        const dy = e.clientY - startPos.current.y;
        const r = startRect.current;
        const x = r.x + dx;
        const y = r.y + dy;

        setSelectionRect({ x, y, w: r.w, h: r.h });
        if (selectionRef.current) {
          selectionRef.current.style.left = `${x}px`;
          selectionRef.current.style.top = `${y}px`;
        }
        return;
      }

      // Handle resize drag
      if (resizeDir && startRect.current) {
        const dx = e.clientX - startPos.current.x;
        const dy = e.clientY - startPos.current.y;
        const r = startRect.current;
        let { x, y, w, h } = r;

        if (resizeDir.includes("n")) {
          y = r.y + dy;
          h = r.h - dy;
        }
        if (resizeDir.includes("s")) {
          h = r.h + dy;
        }
        if (resizeDir.includes("w")) {
          x = r.x + dx;
          w = r.w - dx;
        }
        if (resizeDir.includes("e")) {
          w = r.w + dx;
        }

        // Ensure minimum size
        if (w < 10) { w = 10; x = resizeDir.includes("w") ? r.x + r.w - 10 : x; }
        if (h < 10) { h = 10; y = resizeDir.includes("n") ? r.y + r.h - 10 : y; }

        setSelectionRect({ x, y, w, h });
        if (selectionRef.current) {
          selectionRef.current.style.left = `${x}px`;
          selectionRef.current.style.top = `${y}px`;
          selectionRef.current.style.width = `${w}px`;
          selectionRef.current.style.height = `${h}px`;
        }
        if (sizeRef.current) {
          sizeRef.current.style.left = `${x + w + 8}px`;
          sizeRef.current.style.top = `${y + h - 24}px`;
          sizeRef.current.textContent = `${Math.round(w)} × ${Math.round(h)}`;
          sizeRef.current.style.display = "block";
        }
        return;
      }

      if (!isSelecting) return;

      const x = Math.min(e.clientX, startPos.current.x);
      const y = Math.min(e.clientY, startPos.current.y);
      const w = Math.abs(e.clientX - startPos.current.x);
      const h = Math.abs(e.clientY - startPos.current.y);

      if (selectionRef.current) {
        selectionRef.current.style.left = `${x}px`;
        selectionRef.current.style.top = `${y}px`;
        selectionRef.current.style.width = `${w}px`;
        selectionRef.current.style.height = `${h}px`;
        selectionRef.current.style.display = "block";
      }

      if (sizeRef.current) {
        sizeRef.current.style.left = `${x + w + 8}px`;
        sizeRef.current.style.top = `${y + h - 24}px`;
        sizeRef.current.textContent = `${w} × ${h}`;
        sizeRef.current.style.display = "block";
      }
    },
    [isSelecting, resizeDir, isDragging]
  );

  const handleMouseUp = useCallback(
    async (e: React.MouseEvent) => {
      // Handle drag end
      if (isDragging) {
        setIsDragging(false);
        startRect.current = null;
        setOriginalWindowInfo(null); // Clear window info since user moved the selection
        return;
      }

      // Handle resize end
      if (resizeDir) {
        setResizeDir(null);
        startRect.current = null;
        setOriginalWindowInfo(null); // Clear window info since user resized the selection
        if (sizeRef.current) sizeRef.current.style.display = "none";
        return;
      }

      if (!isSelecting) return;
      setIsSelecting(false);

      endPos.current = { x: e.clientX, y: e.clientY };
      const x = Math.min(e.clientX, startPos.current.x);
      const y = Math.min(e.clientY, startPos.current.y);
      const w = Math.abs(e.clientX - startPos.current.x);
      const h = Math.abs(e.clientY - startPos.current.y);

      if (w > 10 && h > 10) {
        const newRect = { x, y, w, h };
        setSelectionRect(newRect);
        setHoveredWindow(null);
        setOriginalWindowInfo(null);
        if (sizeRef.current) sizeRef.current.style.display = "none";

        // scroll 模式：选区确认后直接进入滚动捕获（内联）
        if (mode === "scroll") {
          const region = {
            x: Math.round(x),
            y: Math.round(y),
            width: Math.round(w),
            height: Math.round(h),
          };
          try {
            await invoke("set_region", { region });
            await invoke("set_selector_mouse_passthrough", { enabled: true });
            const progress = await invoke<{ frame_count: number; total_height: number; preview_base64: string }>(
              "start_scroll_capture_inline",
              { region }
            );
            setScrollPreview(progress.preview_base64);
            setScrollFrameCount(progress.frame_count);
            setScrollTotalHeight(progress.total_height);
            setScrollCaptureActive(true);
          } catch (e) {
            console.error("[Selector] Failed to start scroll capture:", e);
            await invoke("set_selector_mouse_passthrough", { enabled: false });
          }
        } else {
          setShowToolbar(true);
        }
      } else {
        if (selectionRef.current) selectionRef.current.style.display = "none";
        if (sizeRef.current) sizeRef.current.style.display = "none";

        // Use get_window_info_at_cursor to get titlebar height
        const windowInfo = await invoke<WindowInfo | null>("get_window_info_at_cursor");

        if (windowInfo) {
          setCurrentTitlebarHeight(windowInfo.titlebar_height);
          setOriginalWindowInfo(windowInfo);

          const finalY = excludeTitlebar ? windowInfo.y + windowInfo.titlebar_height : windowInfo.y;
          const finalH = excludeTitlebar ? windowInfo.height - windowInfo.titlebar_height : windowInfo.height;

          const newRect = {
            x: windowInfo.x,
            y: finalY,
            w: windowInfo.width,
            h: finalH,
          };
          setSelectionRect(newRect);
          setShowHint(false);
          setHoveredWindow(null);

          if (selectionRef.current) {
            selectionRef.current.style.left = `${windowInfo.x}px`;
            selectionRef.current.style.top = `${finalY}px`;
            selectionRef.current.style.width = `${windowInfo.width}px`;
            selectionRef.current.style.height = `${finalH}px`;
            selectionRef.current.style.display = "block";
          }

          // scroll 模式：选区确认后直接进入滚动捕获（内联）
          if (mode === "scroll") {
            const region = {
              x: Math.round(windowInfo.x),
              y: Math.round(finalY),
              width: Math.round(windowInfo.width),
              height: Math.round(finalH),
            };
            try {
              await invoke("set_region", { region });
              await invoke("set_selector_mouse_passthrough", { enabled: true });
              const progress = await invoke<{ frame_count: number; total_height: number; preview_base64: string }>(
                "start_scroll_capture_inline",
                { region }
              );
              setScrollPreview(progress.preview_base64);
              setScrollFrameCount(progress.frame_count);
              setScrollTotalHeight(progress.total_height);
              setScrollCaptureActive(true);
            } catch (e) {
              console.error("[Selector] Failed to start scroll capture:", e);
              await invoke("set_selector_mouse_passthrough", { enabled: false });
            }
          } else {
            setShowToolbar(true);
          }
        } else {
          setShowHint(true);
        }
      }
    },
    [isSelecting, resizeDir, isDragging, excludeTitlebar, mode, closeWindow]
  );

  // Re-calculate selection when excludeTitlebar changes (only for window selections)
  useEffect(() => {
    if (!originalWindowInfo || !showToolbar) return;

    const finalY = excludeTitlebar
      ? originalWindowInfo.y + originalWindowInfo.titlebar_height
      : originalWindowInfo.y;
    const finalH = excludeTitlebar
      ? originalWindowInfo.height - originalWindowInfo.titlebar_height
      : originalWindowInfo.height;

    setSelectionRect({
      x: originalWindowInfo.x,
      y: finalY,
      w: originalWindowInfo.width,
      h: finalH,
    });

    if (selectionRef.current) {
      selectionRef.current.style.left = `${originalWindowInfo.x}px`;
      selectionRef.current.style.top = `${finalY}px`;
      selectionRef.current.style.width = `${originalWindowInfo.width}px`;
      selectionRef.current.style.height = `${finalH}px`;
    }
  }, [excludeTitlebar, originalWindowInfo, showToolbar]);

  // Toggle between static and dynamic screenshot mode
  const toggleStaticMode = useCallback(async () => {
    if (mode === "staticimage") {
      // Static -> Dynamic: clear background and switch mode
      await invoke("clear_screen_background");
      setMode("image");
    } else if (mode === "image") {
      // Dynamic -> Static: set window background directly (GPU accelerated)
      // This also updates cached_snapshot, so magnifier will use the frozen screen
      const success = await invoke<boolean>("capture_screen_now");
      if (success) {
        setMode("staticimage");
      }
    }
  }, [mode]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      // Check if user is typing in textarea (text annotation input)
      const isTypingInTextarea = (e.target as HTMLElement).tagName === "TEXTAREA";

      // F12 opens devtools (always available)
      if (e.key === "F12") {
        invoke("open_devtools");
        return;
      }

      // If typing in textarea, let the textarea handle ESC/Enter
      if (isTypingInTextarea) {
        return;
      }

      // 滚动捕获模式的快捷键
      if (scrollCaptureActive) {
        if (e.key === "Escape") {
          await cancelScrollCapture();
          return;
        }
        if (e.key === "Enter") {
          await finishScrollCapture();
          return;
        }
        return; // 滚动模式下忽略其他快捷键
      }

      // ESC closes window (when not typing)
      if (e.key === "Escape") {
        await closeWindow();
        return;
      }

      // C copies color, P copies position (when magnifier visible)
      if (!showToolbar && !isEditing && mousePos) {
        if (e.key === "c" || e.key === "C") {
          navigator.clipboard.writeText(currentColorRef.current);
          return;
        }
        if (e.key === "p" || e.key === "P") {
          navigator.clipboard.writeText(`${Math.round(mousePos.x)}, ${Math.round(mousePos.y)}`);
          return;
        }
      }

      // Editing mode shortcuts
      if (isEditing) {
        if ((e.metaKey || e.ctrlKey) && e.key === "z") {
          e.preventDefault();
          if (e.shiftKey) {
            editor.redo();
          } else {
            editor.undo();
          }
          return;
        }
        if (e.key === "Delete" || e.key === "Backspace") {
          if (editor.selectedId) {
            e.preventDefault();
            editor.deleteSelected();
          }
          return;
        }
        // Tool shortcuts in editing mode
        if (e.key === "1") { editor.setActiveTool("select"); return; }
        if (e.key === "2") { editor.setActiveTool("rect"); return; }
        if (e.key === "3") { editor.setActiveTool("mosaic"); return; }
        if (e.key === "4") { editor.setActiveTool("arrow"); return; }
        if (e.key === "5") { editor.setActiveTool("text"); return; }
        if (e.key === "Enter") {
          await doCapture();
          return;
        }
        return;
      }

      // Normal mode shortcuts
      if (e.key === "Shift" && (mode === "image" || mode === "staticimage")) {
        // Shift toggles between static and dynamic screenshot
        await toggleStaticMode();
      } else if ((e.key === "s" || e.key === "S") && mode !== "staticimage") {
        setMode("image");
      } else if ((e.key === "g" || e.key === "G") && mode !== "staticimage") {
        setMode("gif");
      } else if ((e.key === "l" || e.key === "L") && scrollCaptureEnabled && mode !== "staticimage") {
        setMode("scroll");
      } else if (e.key === "t" || e.key === "T") {
        setExcludeTitlebar((prev) => !prev);
      } else if (e.key === "c" || e.key === "C") {
        setCaptionEnabled((prev) => !prev);
      } else if (e.key === "Enter" && selectionRect) {
        await doCapture();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [selectionRect, doCapture, closeWindow, scrollCaptureEnabled, mode, toggleStaticMode, isEditing, editor, exitEditMode, scrollCaptureActive, finishScrollCapture, cancelScrollCapture, showToolbar, mousePos]);

  const toolbarStyle: React.CSSProperties = (() => {
    if (!selectionRect) return {};

    const TOOLBAR_WIDTH = 500;
    const TOOLBAR_HEIGHT = 60;
    const GAP = 12;
    const PADDING = 8;

    const { x, y, w, h } = selectionRect;
    const ep = endPos.current;

    // 计算各个方向的可用空间
    const spaceBottom = window.innerHeight - (y + h + GAP);
    const spaceTop = y - GAP;
    const spaceRight = window.innerWidth - (x + w + GAP);
    const spaceLeft = x - GAP;

    // 计算工具栏中心到终点的距离
    const dist = (cx: number, cy: number) => Math.hypot(cx - ep.x, cy - ep.y);

    // 候选位置：外侧四个方向，工具栏中心尽量靠近终点
    const clampX = (v: number) => Math.max(PADDING, Math.min(v, window.innerWidth - TOOLBAR_WIDTH - PADDING));
    const clampY = (v: number) => Math.max(PADDING, Math.min(v, window.innerHeight - TOOLBAR_HEIGHT - PADDING));

    const candidates: { left: number; top: number; dist: number }[] = [];

    // 下方
    if (spaceBottom >= TOOLBAR_HEIGHT) {
      const l = clampX(ep.x - TOOLBAR_WIDTH / 2);
      const t = y + h + GAP;
      candidates.push({ left: l, top: t, dist: dist(l + TOOLBAR_WIDTH / 2, t + TOOLBAR_HEIGHT / 2) });
    }
    // 上方
    if (spaceTop >= TOOLBAR_HEIGHT) {
      const l = clampX(ep.x - TOOLBAR_WIDTH / 2);
      const t = y - GAP - TOOLBAR_HEIGHT;
      candidates.push({ left: l, top: t, dist: dist(l + TOOLBAR_WIDTH / 2, t + TOOLBAR_HEIGHT / 2) });
    }
    // 右侧
    if (spaceRight >= TOOLBAR_WIDTH) {
      const l = x + w + GAP;
      const t = clampY(ep.y - TOOLBAR_HEIGHT / 2);
      candidates.push({ left: l, top: t, dist: dist(l + TOOLBAR_WIDTH / 2, t + TOOLBAR_HEIGHT / 2) });
    }
    // 左侧
    if (spaceLeft >= TOOLBAR_WIDTH) {
      const l = x - GAP - TOOLBAR_WIDTH;
      const t = clampY(ep.y - TOOLBAR_HEIGHT / 2);
      candidates.push({ left: l, top: t, dist: dist(l + TOOLBAR_WIDTH / 2, t + TOOLBAR_HEIGHT / 2) });
    }

    let left: number, top: number;

    if (candidates.length > 0) {
      // 选择距离终点最近的外侧位置
      const best = candidates.reduce((a, b) => (a.dist < b.dist ? a : b));
      left = best.left;
      top = best.top;
    } else {
      // 外侧都放不下，放在区域内靠近终点的位置
      left = clampX(ep.x - TOOLBAR_WIDTH / 2);
      top = clampY(ep.y - TOOLBAR_HEIGHT / 2);
      // 确保不超出选区且有 GAP
      left = Math.max(x + GAP, Math.min(left, x + w - TOOLBAR_WIDTH - GAP));
      top = Math.max(y + GAP, Math.min(top, y + h - TOOLBAR_HEIGHT - GAP));
    }

    // 边界限制
    left = Math.max(PADDING, Math.min(left, window.innerWidth - TOOLBAR_WIDTH - PADDING));
    top = Math.max(PADDING, Math.min(top, window.innerHeight - TOOLBAR_HEIGHT - PADDING));

    return { left, top };
  })();

  const scrollCaptureUiActive = scrollCaptureActive || (mode === "scroll" && selectionRect);
  const showCrosshair = showHint && !isSelecting && !showToolbar && mousePos && !scrollCaptureUiActive;
  // 窗口高亮在拖拽时保持显示作为参考，只有确认选区后才消失
  const showWindowHighlight = !showToolbar && !selectionRect && hoveredWindow;
  const isStaticMode = mode === "staticimage";

  return (
    <div
      className={`selector-container ${showCrosshair ? "hide-cursor" : ""} ${scrollCaptureActive ? "scroll-capture-mode" : ""}`}
      onMouseDown={scrollCaptureActive ? undefined : handleMouseDown}
      onMouseMove={scrollCaptureActive ? undefined : handleMouseMove}
      onMouseUp={scrollCaptureActive ? undefined : handleMouseUp}
      style={isStaticMode && screenSnapshot ? { background: `url(${screenSnapshot}) no-repeat center/cover` } : undefined}
    >
      {showWindowHighlight && (
        <div
          className="window-highlight"
          style={{
            left: hoveredWindow!.x,
            top: hoveredWindow!.y,
            width: hoveredWindow!.w,
            height: hoveredWindow!.h,
          }}
        />
      )}
      {showCrosshair && (
        <>
          <div className="crosshair-h" style={{ top: mousePos!.y }} />
          <div className="crosshair-v" style={{ left: mousePos!.x }} />
        </>
      )}
      {/* Magnifier - 在选区确认前显示，滚动模式下隐藏 */}
      {!showToolbar && !isEditing && !scrollCaptureUiActive && mousePos && magnifierReady && (
        <Magnifier
          cursorX={mousePos.x}
          cursorY={mousePos.y}
          screenWidth={window.innerWidth}
          screenHeight={window.innerHeight}
          isDragging={isSelecting}
          selectionStart={isSelecting ? startPos.current : undefined}
          onColorChange={(color) => { currentColorRef.current = color; }}
        />
      )}
      <div ref={selectionRef} className="selection" />
      <div ref={sizeRef} className="size-label" />

      {showToolbar && selectionRect && !isEditing && (
        <div
          className="selection-drag-area"
          style={{
            position: "fixed",
            left: selectionRect.x,
            top: selectionRect.y,
            width: selectionRect.w,
            height: selectionRect.h,
            cursor: "move",
            zIndex: 11,
          }}
          onMouseDown={handleDragStart}
        />
      )}

      {showToolbar && selectionRect && (
        <>
          {/* Edge handles */}
          <div
            className="resize-handle resize-n"
            style={{ left: selectionRect.x, top: selectionRect.y - 4, width: selectionRect.w }}
            onMouseDown={handleResizeStart("n")}
          />
          <div
            className="resize-handle resize-s"
            style={{ left: selectionRect.x, top: selectionRect.y + selectionRect.h - 4, width: selectionRect.w }}
            onMouseDown={handleResizeStart("s")}
          />
          <div
            className="resize-handle resize-w"
            style={{ left: selectionRect.x - 4, top: selectionRect.y, height: selectionRect.h }}
            onMouseDown={handleResizeStart("w")}
          />
          <div
            className="resize-handle resize-e"
            style={{ left: selectionRect.x + selectionRect.w - 4, top: selectionRect.y, height: selectionRect.h }}
            onMouseDown={handleResizeStart("e")}
          />
          {/* Corner handles */}
          <div
            className="resize-handle resize-corner resize-nw"
            style={{ left: selectionRect.x - 5, top: selectionRect.y - 5 }}
            onMouseDown={handleResizeStart("nw")}
          />
          <div
            className="resize-handle resize-corner resize-ne"
            style={{ left: selectionRect.x + selectionRect.w - 5, top: selectionRect.y - 5 }}
            onMouseDown={handleResizeStart("ne")}
          />
          <div
            className="resize-handle resize-corner resize-sw"
            style={{ left: selectionRect.x - 5, top: selectionRect.y + selectionRect.h - 5 }}
            onMouseDown={handleResizeStart("sw")}
          />
          <div
            className="resize-handle resize-corner resize-se"
            style={{ left: selectionRect.x + selectionRect.w - 5, top: selectionRect.y + selectionRect.h - 5 }}
            onMouseDown={handleResizeStart("se")}
          />
        </>
      )}


      {showToolbar && (
        <div id="toolbar" className="toolbar" style={toolbarStyle}>
          {/* 模式组 */}
          <div className="toolbar-section">
            <span className="toolbar-section-title">模式</span>
            <div className="toolbar-section-content">
              <button
                className={`toolbar-btn has-tooltip ${mode === "image" || mode === "staticimage" ? "active" : ""}`}
                onClick={isStaticMode ? undefined : () => setMode("image")}
                disabled={isStaticMode}
                style={isStaticMode ? { cursor: "default" } : undefined}
                data-tooltip={isStaticMode ? "静态截图模式" : "截图 (S) - 按 Shift 切换静态/动态"}
              >
                S
              </button>
              <button
                className={`toolbar-btn has-tooltip ${mode === "gif" ? "active" : ""}`}
                onClick={isStaticMode ? undefined : () => setMode("gif")}
                disabled={isStaticMode}
                style={isStaticMode ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
                data-tooltip={isStaticMode ? "静态模式下不可用" : "录制 GIF (G) - 选区内录制动画"}
              >
                G
              </button>
              <button
                className={`toolbar-btn has-tooltip ${mode === "scroll" ? "active" : ""}`}
                onClick={scrollCaptureEnabled && !isStaticMode ? () => setMode("scroll") : undefined}
                disabled={!scrollCaptureEnabled || isStaticMode}
                style={!scrollCaptureEnabled || isStaticMode ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
                data-tooltip={isStaticMode ? "静态模式下不可用" : scrollCaptureEnabled ? "滚动截图 (L) - 自动拼接长图" : "滚动截图 (L) - 需在设置中启用"}
              >
                L
              </button>
              <button
                className="toolbar-btn has-tooltip"
                disabled
                style={{ opacity: 0.4, cursor: "not-allowed" }}
                data-tooltip="录制视频 (V) - 即将推出"
              >
                V
              </button>
            </div>
          </div>

          <div className="toolbar-divider" />

          {/* 标注组 */}
          <div className="toolbar-section">
            <span className="toolbar-section-title">标注</span>
            <div className="toolbar-section-content">
              <div className="toolbar-group">
                <button
                  className={`toolbar-btn has-tooltip ${isEditing && editor.activeTool === "rect" ? "active" : ""}`}
                  onClick={() => isEditing ? editor.setActiveTool("rect") : enterEditMode("rect")}
                  data-tooltip="矩形标注 (2)"
                >
                  □
                </button>
                {isEditing && editor.activeTool === "rect" && (
                  <div className="tool-dropdown">
                    <div className="dropdown-styles">
                      {STYLE_OPTIONS.rect.map((opt) => (
                        <button
                          key={opt.value}
                          className={`dropdown-item ${editor.activeStyles.rect === opt.value ? "active" : ""}`}
                          onClick={() => editor.setRectStyle(opt.value as typeof editor.activeStyles.rect)}
                        >
                          <span className="dropdown-icon">{opt.icon}</span>
                          <span>{opt.label}</span>
                        </button>
                      ))}
                    </div>
                    <div className="dropdown-divider" />
                    <div className="dropdown-colors">
                      {ANNOTATION_COLORS.map((c) => (
                        <button
                          key={c.name}
                          className={`color-dot ${editor.activeColor === c.value ? "active" : ""}`}
                          style={{ backgroundColor: c.value }}
                          onClick={() => editor.setActiveColor(c.value)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>

              <div className="toolbar-group">
                <button
                  className={`toolbar-btn has-tooltip ${isEditing && editor.activeTool === "mosaic" ? "active" : ""}`}
                  onClick={() => isEditing ? editor.setActiveTool("mosaic") : enterEditMode("mosaic")}
                  data-tooltip="马赛克 (3)"
                >
                  ▦
                </button>
                {isEditing && editor.activeTool === "mosaic" && (
                  <div className="tool-dropdown">
                    <div className="dropdown-styles">
                      {STYLE_OPTIONS.mosaic.map((opt) => (
                        <button
                          key={opt.value}
                          className={`dropdown-item ${editor.activeStyles.mosaic === opt.value ? "active" : ""}`}
                          onClick={() => editor.setMosaicStyle(opt.value as typeof editor.activeStyles.mosaic)}
                        >
                          <span className="dropdown-icon">{opt.icon}</span>
                          <span>{opt.label}</span>
                        </button>
                      ))}
                    </div>
                  </div>
                )}
              </div>

              <div className="toolbar-group">
                <button
                  className={`toolbar-btn has-tooltip ${isEditing && editor.activeTool === "arrow" ? "active" : ""}`}
                  onClick={() => isEditing ? editor.setActiveTool("arrow") : enterEditMode("arrow")}
                  data-tooltip="箭头 (4)"
                >
                  →
                </button>
                {isEditing && editor.activeTool === "arrow" && (
                  <div className="tool-dropdown">
                    <div className="dropdown-styles">
                      {STYLE_OPTIONS.arrow.map((opt) => (
                        <button
                          key={opt.value}
                          className={`dropdown-item ${editor.activeStyles.arrow === opt.value ? "active" : ""}`}
                          onClick={() => editor.setArrowStyle(opt.value as typeof editor.activeStyles.arrow)}
                        >
                          <span className="dropdown-icon">{opt.icon}</span>
                          <span>{opt.label}</span>
                        </button>
                      ))}
                    </div>
                    <div className="dropdown-divider" />
                    <div className="dropdown-colors">
                      {ANNOTATION_COLORS.map((c) => (
                        <button
                          key={c.name}
                          className={`color-dot ${editor.activeColor === c.value ? "active" : ""}`}
                          style={{ backgroundColor: c.value }}
                          onClick={() => editor.setActiveColor(c.value)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>

              <div className="toolbar-group">
                <button
                  className={`toolbar-btn has-tooltip ${isEditing && editor.activeTool === "text" ? "active" : ""}`}
                  onClick={() => isEditing ? editor.setActiveTool("text") : enterEditMode("text")}
                  data-tooltip="文字 (5)"
                >
                  T
                </button>
                {isEditing && editor.activeTool === "text" && (
                  <div className="tool-dropdown">
                    <div className="dropdown-colors">
                      {ANNOTATION_COLORS.map((c) => (
                        <button
                          key={c.name}
                          className={`color-dot ${editor.activeColor === c.value ? "active" : ""}`}
                          style={{ backgroundColor: c.value }}
                          onClick={() => editor.setActiveColor(c.value)}
                        />
                      ))}
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* 历史组 (only in editing mode) */}
          {isEditing && (
            <>
              <div className="toolbar-divider" />
              <div className="toolbar-section">
                <span className="toolbar-section-title">历史</span>
                <div className="toolbar-section-content">
                  <button
                    className="toolbar-btn has-tooltip"
                    onClick={editor.undo}
                    disabled={!editor.canUndo}
                    style={!editor.canUndo ? { opacity: 0.4 } : undefined}
                    data-tooltip="撤销 (⌘Z)"
                  >
                    ↩
                  </button>
                  <button
                    className="toolbar-btn has-tooltip"
                    onClick={editor.redo}
                    disabled={!editor.canRedo}
                    style={!editor.canRedo ? { opacity: 0.4 } : undefined}
                    data-tooltip="重做 (⌘⇧Z)"
                  >
                    ↪
                  </button>
                </div>
              </div>
            </>
          )}

          {/* 选项组 (hidden in editing mode) */}
          {!isEditing && (
            <>
              <div className="toolbar-divider" />
              <div className="toolbar-section">
                <span className="toolbar-section-title">选项</span>
                <div className="toolbar-section-content">
                  <button
                    className={`toolbar-btn has-tooltip ${excludeTitlebar ? "active" : ""}`}
                    onClick={() => setExcludeTitlebar(!excludeTitlebar)}
                    data-tooltip={`排除标题栏 (T) - ${currentTitlebarHeight}px`}
                  >
                    T
                  </button>
                  <button
                    className={`toolbar-btn has-tooltip ${captionEnabled ? "active" : ""}`}
                    onClick={() => setCaptionEnabled(!captionEnabled)}
                    data-tooltip="添加描述 (C) - 截图后输入说明文字"
                  >
                    C
                  </button>
                </div>
              </div>
            </>
          )}

          <div className="toolbar-divider" />

          {/* 操作组 */}
          <div className="toolbar-section">
            <span className="toolbar-section-title">操作</span>
            <div className="toolbar-section-content">
              <button
                className="toolbar-btn has-tooltip"
                onClick={(e) => {
                  e.stopPropagation();
                  doCapture();
                }}
                data-tooltip="确认 (Enter)"
              >
                ✓
              </button>
              <button
                className="toolbar-btn has-tooltip"
                onClick={closeWindow}
                data-tooltip="取消 (ESC)"
              >
                X
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Annotation Canvas */}
      {isEditing && previewImage && selectionRect && (
        <AnnotationCanvas
          imageUrl={previewImage}
          width={selectionRect.w}
          height={selectionRect.h}
          left={selectionRect.x}
          top={selectionRect.y}
          annotations={editor.annotations}
          selectedId={editor.selectedId}
          activeTool={editor.activeTool}
          activeColor={editor.activeColor}
          activeStyles={editor.activeStyles}
          strokeWidth={editor.strokeWidth}
          fontSize={editor.fontSize}
          onAddAnnotation={editor.addAnnotation}
          onUpdateAnnotation={editor.updateAnnotation}
          onDeleteAnnotation={editor.deleteAnnotation}
          onSelectAnnotation={editor.setSelectedId}
          stageRef={stageRef}
        />
      )}

      {/* Scroll Capture Mode UI */}
      {scrollCaptureActive && selectionRect && (
        <>
          {/* 选区边框 - 四角标记 */}
          <div
            className="scroll-capture-border"
            style={{
              position: "fixed",
              left: selectionRect.x - 2,
              top: selectionRect.y - 2,
              width: selectionRect.w + 4,
              height: selectionRect.h + 4,
              border: "2px dashed #CC785C",
              borderRadius: 4,
              pointerEvents: "none",
              zIndex: 100,
            }}
          >
            {/* 四角标记 */}
            <div className="corner-mark corner-tl" />
            <div className="corner-mark corner-tr" />
            <div className="corner-mark corner-bl" />
            <div className="corner-mark corner-br" />
          </div>

          {/* 底部预览条 - 鼠标进入时禁用穿透，离开时启用穿透 */}
          <div
            className="scroll-capture-bar"
            onMouseEnter={async () => {
              await invoke("set_selector_mouse_passthrough", { enabled: false });
            }}
            onMouseLeave={async () => {
              await invoke("set_selector_mouse_passthrough", { enabled: true });
            }}
            style={{
              position: "fixed",
              left: 0,
              right: 0,
              bottom: 0,
              height: 80,
              background: "rgba(24, 24, 24, 0.95)",
              borderTop: "1px solid rgba(204, 120, 92, 0.3)",
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              gap: 24,
              zIndex: 200,
            }}
          >
            {/* 缩略图预览 */}
            {scrollPreview && (
              <div
                style={{
                  height: 60,
                  maxWidth: 200,
                  borderRadius: 4,
                  overflow: "hidden",
                  border: "1px solid rgba(255,255,255,0.2)",
                }}
              >
                <img
                  src={scrollPreview}
                  alt=""
                  style={{ height: "100%", width: "auto", objectFit: "contain" }}
                  draggable={false}
                />
              </div>
            )}

            {/* 统计信息 */}
            <div style={{ color: "#fff", fontSize: 14 }}>
              <span style={{ color: "#CC785C", fontWeight: 600 }}>{scrollFrameCount}</span> 帧
              <span style={{ margin: "0 8px", opacity: 0.5 }}>·</span>
              <span style={{ color: "#CC785C", fontWeight: 600 }}>{scrollTotalHeight}</span> px
            </div>

            {/* 操作按钮 */}
            <div style={{ display: "flex", gap: 8 }}>
              <button
                onMouseDown={finishScrollCapture}
                style={{
                  padding: "8px 16px",
                  background: "#CC785C",
                  color: "#fff",
                  border: "none",
                  borderRadius: 6,
                  fontSize: 13,
                  fontWeight: 500,
                  cursor: "pointer",
                }}
              >
                完成
              </button>
              <button
                onMouseDown={cancelScrollCapture}
                style={{
                  padding: "8px 16px",
                  background: "rgba(255,255,255,0.1)",
                  color: "#fff",
                  border: "1px solid rgba(255,255,255,0.2)",
                  borderRadius: 6,
                  fontSize: 13,
                  fontWeight: 500,
                  cursor: "pointer",
                }}
              >
                取消
              </button>
            </div>

            {/* 快捷键提示 */}
            <div style={{ color: "rgba(255,255,255,0.5)", fontSize: 11 }}>
              <kbd style={{ background: "rgba(255,255,255,0.1)", padding: "2px 6px", borderRadius: 3, marginRight: 4 }}>Enter</kbd>
              /
              <kbd style={{ background: "rgba(255,255,255,0.1)", padding: "2px 6px", borderRadius: 3, margin: "0 4px" }}>ESC</kbd>
            </div>
          </div>
        </>
      )}
    </div>
  );
}
