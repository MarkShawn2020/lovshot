import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

type Mode = "image" | "gif" | "video" | "scroll";

interface ScrollCaptureProgress {
  frame_count: number;
  total_height: number;
  preview_base64: string;
}

interface SelectionRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export default function Selector() {
  const [isSelecting, setIsSelecting] = useState(false);
  const [selectionRect, setSelectionRect] = useState<SelectionRect | null>(null);
  const [mode, setMode] = useState<Mode>("image");
  const [showHint, setShowHint] = useState(true);
  const [showToolbar, setShowToolbar] = useState(false);
  const [mousePos, setMousePos] = useState<{ x: number; y: number } | null>(null);
  const [hoveredWindow, setHoveredWindow] = useState<SelectionRect | null>(null);
  // Scroll capture state
  const [isScrollCapturing, setIsScrollCapturing] = useState(false);
  const [scrollProgress, setScrollProgress] = useState<ScrollCaptureProgress | null>(null);

  const startPos = useRef({ x: 0, y: 0 });
  const selectionRef = useRef<HTMLDivElement>(null);
  const sizeRef = useRef<HTMLDivElement>(null);
  const lastDetectTime = useRef(0);

  const closeWindow = useCallback(async () => {
    await getCurrentWindow().close();
  }, []);

  // Fetch pending mode from backend on mount
  useEffect(() => {
    invoke<Mode | null>("get_pending_mode").then((pendingMode) => {
      console.log("[Selector] get_pending_mode 返回:", pendingMode);
      if (pendingMode) {
        setMode(pendingMode);
        // Clear it after reading
        invoke("clear_pending_mode");
      }
    });
  }, []);

  // Track mouse position and detect window under cursor (throttled)
  useEffect(() => {
    const handler = async (e: MouseEvent) => {
      setMousePos({ x: e.clientX, y: e.clientY });

      // Only detect window when in hover mode (not selecting, not showing toolbar)
      if (isSelecting || showToolbar) return;

      // Throttle: max once per 50ms
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

    // Get initial mouse position and window from Rust
    invoke<[number, number] | null>("get_mouse_position").then((pos) => {
      if (pos) setMousePos({ x: pos[0], y: pos[1] });
    });
    invoke<{ x: number; y: number; width: number; height: number } | null>("get_window_at_cursor").then((win) => {
      if (win) setHoveredWindow({ x: win.x, y: win.y, w: win.width, h: win.height });
    });

    return () => document.removeEventListener("mousemove", handler);
  }, [isSelecting, showToolbar]);

  const doCapture = useCallback(async () => {
    console.log("[Selector] doCapture called, mode:", mode, "selectionRect:", selectionRect);
    if (!selectionRect) return;

    const region = {
      x: Math.round(selectionRect.x),
      y: Math.round(selectionRect.y),
      width: Math.round(selectionRect.w),
      height: Math.round(selectionRect.h),
    };

    await invoke("set_region", { region });

    if (mode === "image") {
      // Hide entire window before screenshot to avoid capturing UI
      const win = getCurrentWindow();
      await win.hide();
      // Wait for window to fully hide
      await new Promise((r) => setTimeout(r, 50));
      await invoke("save_screenshot");
      await win.close();
    } else if (mode === "gif") {
      await invoke("start_recording");
      await closeWindow();
    } else if (mode === "scroll") {
      // Start scroll capture mode - make window click-through to allow scrolling underlying content
      console.log("[Selector] 进入 scroll 模式");
      const win = getCurrentWindow();

      setShowToolbar(false);
      setIsScrollCapturing(true);

      try {
        // Make window ignore cursor events so user can scroll underlying content
        console.log("[Selector] 设置窗口穿透...");
        await win.setIgnoreCursorEvents(true);

        console.log("[Selector] 调用 start_scroll_capture...");
        const progress = await invoke<ScrollCaptureProgress>("start_scroll_capture");
        console.log("[Selector] start_scroll_capture 成功:", progress);
        setScrollProgress(progress);
      } catch (e) {
        console.error("[Selector] Failed to start scroll capture:", e);
        setIsScrollCapturing(false);
        await win.setIgnoreCursorEvents(false);
      }
    }
  }, [selectionRect, mode, closeWindow]);

  // Finish scroll capture
  const finishScrollCapture = useCallback(async () => {
    if (!isScrollCapturing) return;
    try {
      const win = getCurrentWindow();
      await win.setIgnoreCursorEvents(false);
      await win.hide();
      await new Promise((r) => setTimeout(r, 50));
      await invoke<string>("finish_scroll_capture");
      await win.close();
    } catch (e) {
      console.error("Failed to finish scroll capture:", e);
    }
  }, [isScrollCapturing]);

  // Cancel scroll capture
  const cancelScrollCapture = useCallback(async () => {
    if (!isScrollCapturing) return;
    const win = getCurrentWindow();
    await win.setIgnoreCursorEvents(false);
    await invoke("cancel_scroll_capture");
    setIsScrollCapturing(false);
    setScrollProgress(null);
    setShowToolbar(true);
  }, [isScrollCapturing]);

  // Poll for scroll changes (since overlay window blocks wheel events to underlying windows)
  useEffect(() => {
    if (!isScrollCapturing) return;

    let isCapturing = false;
    const POLL_INTERVAL = 200; // ms

    const pollCapture = async () => {
      if (isCapturing) return;
      isCapturing = true;

      try {
        // Use auto-detect mode - backend will compare with previous frame
        const progress = await invoke<ScrollCaptureProgress | null>("capture_scroll_frame_auto");
        if (progress) {
          setScrollProgress(progress);
        }
      } catch (e) {
        // Ignore errors during polling
      } finally {
        isCapturing = false;
      }
    };

    const intervalId = setInterval(pollCapture, POLL_INTERVAL);
    return () => clearInterval(intervalId);
  }, [isScrollCapturing]);

  // Listen for global shortcut to finish scroll capture
  useEffect(() => {
    if (!isScrollCapturing) return;

    const unlisten = listen("scroll-capture-finish", () => {
      finishScrollCapture();
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [isScrollCapturing, finishScrollCapture]);

  // Mouse events
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("#toolbar")) return;

    setShowToolbar(false);
    setSelectionRect(null);
    setShowHint(false);
    setHoveredWindow(null);

    startPos.current = { x: e.clientX, y: e.clientY };
    setIsSelecting(true);
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
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
        sizeRef.current.style.top = `${y + 8}px`;
        sizeRef.current.textContent = `${w} × ${h}`;
        sizeRef.current.style.display = "block";
      }
    },
    [isSelecting]
  );

  const handleMouseUp = useCallback(
    async (e: React.MouseEvent) => {
      if (!isSelecting) return;
      setIsSelecting(false);

      const x = Math.min(e.clientX, startPos.current.x);
      const y = Math.min(e.clientY, startPos.current.y);
      const w = Math.abs(e.clientX - startPos.current.x);
      const h = Math.abs(e.clientY - startPos.current.y);

      if (w > 10 && h > 10) {
        // Drag mode: use dragged region
        setSelectionRect({ x, y, w, h });
        setShowToolbar(true);
        if (sizeRef.current) sizeRef.current.style.display = "none";
      } else {
        // Click mode: detect window under cursor
        if (selectionRef.current) selectionRef.current.style.display = "none";
        if (sizeRef.current) sizeRef.current.style.display = "none";

        const windowRegion = await invoke<{ x: number; y: number; width: number; height: number } | null>(
          "get_window_at_cursor"
        );

        if (windowRegion) {
          setSelectionRect({
            x: windowRegion.x,
            y: windowRegion.y,
            w: windowRegion.width,
            h: windowRegion.height,
          });
          setShowToolbar(true);
          setShowHint(false);

          // Update visual selection box
          if (selectionRef.current) {
            selectionRef.current.style.left = `${windowRegion.x}px`;
            selectionRef.current.style.top = `${windowRegion.y}px`;
            selectionRef.current.style.width = `${windowRegion.width}px`;
            selectionRef.current.style.height = `${windowRegion.height}px`;
            selectionRef.current.style.display = "block";
          }
        } else {
          setShowHint(true);
        }
      }
    },
    [isSelecting]
  );

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (isScrollCapturing) {
        // In scroll capture mode: Enter to finish, ESC to cancel
        if (e.key === "Escape") {
          await cancelScrollCapture();
        } else if (e.key === "Enter") {
          await finishScrollCapture();
        }
        return;
      }

      if (e.key === "Escape") {
        await closeWindow();
      } else if (e.key === "s" || e.key === "S") {
        setMode("image");
      } else if (e.key === "g" || e.key === "G") {
        setMode("gif");
      } else if (e.key === "l" || e.key === "L") {
        setMode("scroll");
      } else if (e.key === "Enter" && selectionRect) {
        await doCapture();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [selectionRect, doCapture, closeWindow, isScrollCapturing, cancelScrollCapture, finishScrollCapture]);

  const toolbarStyle: React.CSSProperties = selectionRect
    ? {
        left: Math.max(10, Math.min(selectionRect.x + selectionRect.w / 2 - 100, window.innerWidth - 220)),
        top: Math.min(selectionRect.y + selectionRect.h + 12, window.innerHeight - 60),
      }
    : {};

  const showCrosshair = showHint && !isSelecting && !showToolbar && !isScrollCapturing && mousePos;
  const showWindowHighlight = showHint && !isSelecting && !showToolbar && !isScrollCapturing && hoveredWindow;

  // Calculate preview panel position (right of selection, or left if not enough space)
  const previewStyle: React.CSSProperties = selectionRect
    ? (() => {
        const previewWidth = 220;
        const rightSpace = window.innerWidth - (selectionRect.x + selectionRect.w);
        const useRight = rightSpace >= previewWidth + 20;
        return {
          left: useRight ? selectionRect.x + selectionRect.w + 12 : selectionRect.x - previewWidth - 12,
          top: selectionRect.y,
          maxHeight: Math.min(selectionRect.h + 100, window.innerHeight - selectionRect.y - 20),
        };
      })()
    : {};

  return (
    <div
      className={`selector-container ${showCrosshair ? "hide-cursor" : ""}`}
      onMouseDown={isScrollCapturing ? undefined : handleMouseDown}
      onMouseMove={isScrollCapturing ? undefined : handleMouseMove}
      onMouseUp={isScrollCapturing ? undefined : handleMouseUp}
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
      <div ref={selectionRef} className={`selection ${isScrollCapturing ? "scroll-active" : ""}`} />
      <div ref={sizeRef} className="size-label" />

      {showHint && (
        <div className="hint">
          Drag to select area. Press <kbd>ESC</kbd> to cancel.
        </div>
      )}

      {showToolbar && (
        <div id="toolbar" className="toolbar" style={toolbarStyle}>
          <button
            className={`toolbar-btn ${mode === "image" ? "active" : ""}`}
            onClick={() => setMode("image")}
            title="Screenshot (S)"
          >
            📷
          </button>
          <button
            className={`toolbar-btn ${mode === "gif" ? "active" : ""}`}
            onClick={() => setMode("gif")}
            title="Record GIF (G)"
          >
            🎬
          </button>
          <button
            className={`toolbar-btn ${mode === "scroll" ? "active" : ""}`}
            onClick={() => setMode("scroll")}
            title="Scroll Capture (L)"
          >
            📜
          </button>
          <button
            className="toolbar-btn"
            disabled
            style={{ opacity: 0.4, cursor: "not-allowed" }}
            title="Record Video (V)"
          >
            🎥
          </button>
          <div className="toolbar-divider" />
          <button
            className="toolbar-btn"
            onClick={(e) => {
              e.stopPropagation();
              doCapture();
            }}
            title="Confirm (Enter)"
          >
            ✓
          </button>
          <button className="toolbar-btn" onClick={closeWindow} title="Cancel (ESC)">
            ✕
          </button>
        </div>
      )}

      {/* Scroll capture preview panel */}
      {isScrollCapturing && scrollProgress && selectionRect && (
        <div className="scroll-preview-panel" style={previewStyle}>
          <div className="scroll-preview-header">
            <span>Scroll to capture</span>
            <span className="scroll-stats">
              {scrollProgress.frame_count} frames · {scrollProgress.total_height}px
            </span>
          </div>
          <div className="scroll-preview-image">
            <img
              src={scrollProgress.preview_base64}
              alt="Scroll preview"
              style={{ maxWidth: "100%", maxHeight: "300px", objectFit: "contain" }}
            />
          </div>
          <div className="scroll-hint">
            Scroll content below, then<br />
            press <kbd>⌥S</kbd> again to save
          </div>
        </div>
      )}
    </div>
  );
}
