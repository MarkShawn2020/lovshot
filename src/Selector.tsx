import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
  const [excludeTitlebar, setExcludeTitlebar] = useState(false);
  const [currentTitlebarHeight, setCurrentTitlebarHeight] = useState(0);
  const [originalWindowInfo, setOriginalWindowInfo] = useState<WindowInfo | null>(null);
  const [scrollCaptureEnabled, setScrollCaptureEnabled] = useState(false);
  const [screenSnapshot, setScreenSnapshot] = useState<string | null>(null);

  const startPos = useRef({ x: 0, y: 0 });
  const startRect = useRef<SelectionRect | null>(null);
  const selectionRef = useRef<HTMLDivElement>(null);
  const sizeRef = useRef<HTMLDivElement>(null);
  const lastDetectTime = useRef(0);

  const closeWindow = useCallback(async () => {
    await getCurrentWindow().close();
  }, []);

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
    console.log("[Selector] doCapture called, mode:", mode, "selectionRect:", selectionRect);
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
      await win.hide();
      // Static mode doesn't need delay since we use cached screenshot
      if (mode === "image") {
        await new Promise((r) => setTimeout(r, 50));
      }
      await invoke("save_screenshot", { useCached: mode === "staticimage" });
      await win.close();
    } else if (mode === "gif") {
      await invoke("start_recording");
      await closeWindow();
    } else if (mode === "scroll") {
      // Scroll mode: open overlays first (captures initial frame + shows UI), then hide selector
      console.log("[Selector] 进入 scroll 模式");
      const win = getCurrentWindow();

      try {
        // Open scroll UI - this internally:
        // 1. Shows region border immediately (selection preserved)
        // 2. Captures initial frame (no delay)
        // 3. Opens preview window with data ready
        await invoke("open_scroll_overlay", { region });

        // Hide selector so it doesn't intercept scroll events
        await win.hide();

        await win.close();
      } catch (e) {
        console.error("[Selector] Failed to start scroll capture:", e);
        try {
          await win.show();
        } catch {
          // ignore
        }
      }
    }
  }, [selectionRect, mode, closeWindow]);

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

  // Mouse events
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("#toolbar")) return;
    if ((e.target as HTMLElement).closest(".resize-handle")) return;

    setShowToolbar(false);
    setSelectionRect(null);
    setShowHint(false);
    // 不立即清除 hoveredWindow，让窗口高亮在拖拽时保持显示作为参考

    startPos.current = { x: e.clientX, y: e.clientY };
    setIsSelecting(true);
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
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
        sizeRef.current.style.top = `${y + 8}px`;
        sizeRef.current.textContent = `${w} × ${h}`;
        sizeRef.current.style.display = "block";
      }
    },
    [isSelecting, resizeDir]
  );

  const handleMouseUp = useCallback(
    async (e: React.MouseEvent) => {
      // Handle resize end
      if (resizeDir) {
        setResizeDir(null);
        startRect.current = null;
        return;
      }

      if (!isSelecting) return;
      setIsSelecting(false);

      const x = Math.min(e.clientX, startPos.current.x);
      const y = Math.min(e.clientY, startPos.current.y);
      const w = Math.abs(e.clientX - startPos.current.x);
      const h = Math.abs(e.clientY - startPos.current.y);

      if (w > 10 && h > 10) {
        setSelectionRect({ x, y, w, h });
        setShowToolbar(true);
        setHoveredWindow(null); // 用户拖拽了自定义区域，清除窗口预览
        setOriginalWindowInfo(null); // Clear window info since user dragged custom area
        if (sizeRef.current) sizeRef.current.style.display = "none";
      } else {
        if (selectionRef.current) selectionRef.current.style.display = "none";
        if (sizeRef.current) sizeRef.current.style.display = "none";

        // Use get_window_info_at_cursor to get titlebar height
        const windowInfo = await invoke<WindowInfo | null>("get_window_info_at_cursor");

        if (windowInfo) {
          setCurrentTitlebarHeight(windowInfo.titlebar_height);
          setOriginalWindowInfo(windowInfo); // Store for later toggle

          // Apply excludeTitlebar if enabled
          const finalY = excludeTitlebar ? windowInfo.y + windowInfo.titlebar_height : windowInfo.y;
          const finalH = excludeTitlebar ? windowInfo.height - windowInfo.titlebar_height : windowInfo.height;

          setSelectionRect({
            x: windowInfo.x,
            y: finalY,
            w: windowInfo.width,
            h: finalH,
          });
          setShowToolbar(true);
          setShowHint(false);
          setHoveredWindow(null);

          if (selectionRef.current) {
            selectionRef.current.style.left = `${windowInfo.x}px`;
            selectionRef.current.style.top = `${finalY}px`;
            selectionRef.current.style.width = `${windowInfo.width}px`;
            selectionRef.current.style.height = `${finalH}px`;
            selectionRef.current.style.display = "block";
          }
        } else {
          setShowHint(true);
        }
      }
    },
    [isSelecting, resizeDir, excludeTitlebar]
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
      const success = await invoke<boolean>("capture_screen_now");
      if (success) {
        setMode("staticimage");
      }
    }
  }, [mode]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        await closeWindow();
      } else if (e.key === "Shift" && (mode === "image" || mode === "staticimage")) {
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
      } else if (e.key === "Enter" && selectionRect) {
        await doCapture();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [selectionRect, doCapture, closeWindow, scrollCaptureEnabled, mode, toggleStaticMode]);

  const toolbarStyle: React.CSSProperties = selectionRect
    ? {
        left: Math.max(10, Math.min(selectionRect.x + selectionRect.w / 2 - 100, window.innerWidth - 220)),
        top: Math.min(selectionRect.y + selectionRect.h + 12, window.innerHeight - 60),
      }
    : {};

  const showCrosshair = showHint && !isSelecting && !showToolbar && mousePos;
  // 窗口高亮在拖拽时保持显示作为参考，只有确认选区后才消失
  const showWindowHighlight = !showToolbar && !selectionRect && hoveredWindow;
  const isStaticMode = mode === "staticimage";

  return (
    <div
      className={`selector-container ${showCrosshair ? "hide-cursor" : ""}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
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
      <div ref={selectionRef} className="selection" />
      <div ref={sizeRef} className="size-label" />

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

      {showHint && (
        <div className="hint">
          Drag to select area. Press <kbd>ESC</kbd> to cancel.
        </div>
      )}

      {showToolbar && (
        <div id="toolbar" className="toolbar" style={toolbarStyle}>
          <button
            className={`toolbar-btn ${mode === "image" || mode === "staticimage" ? "active" : ""}`}
            onClick={isStaticMode ? undefined : () => setMode("image")}
            disabled={isStaticMode}
            style={isStaticMode ? { cursor: "default" } : undefined}
            title={isStaticMode ? "Static Screenshot" : "Screenshot (S)"}
          >
            S
          </button>
          <button
            className={`toolbar-btn ${mode === "gif" ? "active" : ""}`}
            onClick={isStaticMode ? undefined : () => setMode("gif")}
            disabled={isStaticMode}
            style={isStaticMode ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
            title={isStaticMode ? "Not available in static mode" : "Record GIF (G)"}
          >
            G
          </button>
          <button
            className={`toolbar-btn ${mode === "scroll" ? "active" : ""}`}
            onClick={scrollCaptureEnabled && !isStaticMode ? () => setMode("scroll") : undefined}
            disabled={!scrollCaptureEnabled || isStaticMode}
            style={!scrollCaptureEnabled || isStaticMode ? { opacity: 0.4, cursor: "not-allowed" } : undefined}
            title={isStaticMode ? "Not available in static mode" : scrollCaptureEnabled ? "Scroll Capture (L)" : "Scroll Capture (L) - Enable in Settings"}
          >
            L
          </button>
          <button
            className="toolbar-btn"
            disabled
            style={{ opacity: 0.4, cursor: "not-allowed" }}
            title="Record Video (V)"
          >
            V
          </button>
          <div className="toolbar-divider" />
          <button
            className={`toolbar-btn ${excludeTitlebar ? "active" : ""}`}
            onClick={() => setExcludeTitlebar(!excludeTitlebar)}
            title={`Exclude Titlebar (T) - ${currentTitlebarHeight}px`}
          >
            T
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
            OK
          </button>
          <button className="toolbar-btn" onClick={closeWindow} title="Cancel (ESC)">
            X
          </button>
        </div>
      )}
    </div>
  );
}
