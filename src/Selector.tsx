import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Mode = "image" | "gif" | "video" | "scroll";

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
        invoke("clear_pending_mode");
      }
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

    if (mode === "image") {
      const win = getCurrentWindow();
      await win.hide();
      await new Promise((r) => setTimeout(r, 50));
      await invoke("save_screenshot");
      await win.close();
    } else if (mode === "gif") {
      await invoke("start_recording");
      await closeWindow();
    } else if (mode === "scroll") {
      // Scroll mode: hide selector, then open overlays and start capturing
      console.log("[Selector] 进入 scroll 模式");
      const win = getCurrentWindow();

      try {
        // Hide selector so it doesn't intercept scroll events (keep JS context alive)
        await win.hide();
        await new Promise((r) => setTimeout(r, 50));

        // Start capturing first so the preview has data immediately
        await invoke("start_scroll_capture");

        // Open scroll UI (selection overlay + preview panel)
        await invoke("open_scroll_overlay", { region });

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
        setSelectionRect({ x, y, w, h });
        setShowToolbar(true);
        if (sizeRef.current) sizeRef.current.style.display = "none";
      } else {
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
  }, [selectionRect, doCapture, closeWindow]);

  const toolbarStyle: React.CSSProperties = selectionRect
    ? {
        left: Math.max(10, Math.min(selectionRect.x + selectionRect.w / 2 - 100, window.innerWidth - 220)),
        top: Math.min(selectionRect.y + selectionRect.h + 12, window.innerHeight - 60),
      }
    : {};

  const showCrosshair = showHint && !isSelecting && !showToolbar && mousePos;
  const showWindowHighlight = showHint && !isSelecting && !showToolbar && hoveredWindow;

  return (
    <div
      className={`selector-container ${showCrosshair ? "hide-cursor" : ""}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
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
            S
          </button>
          <button
            className={`toolbar-btn ${mode === "gif" ? "active" : ""}`}
            onClick={() => setMode("gif")}
            title="Record GIF (G)"
          >
            G
          </button>
          <button
            className={`toolbar-btn ${mode === "scroll" ? "active" : ""}`}
            onClick={() => setMode("scroll")}
            title="Scroll Capture (L)"
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
