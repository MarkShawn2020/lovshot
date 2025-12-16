import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type Mode = "image" | "gif" | "video";
type OutputScale = 1 | 0.75 | 0.5 | 0.25;

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
  const [outputScale, setOutputScale] = useState<OutputScale>(1);
  const [mousePos, setMousePos] = useState<{ x: number; y: number } | null>(null);

  const startPos = useRef({ x: 0, y: 0 });
  const selectionRef = useRef<HTMLDivElement>(null);
  const sizeRef = useRef<HTMLDivElement>(null);

  const closeWindow = useCallback(async () => {
    await getCurrentWindow().close();
  }, []);

  // Track mouse position globally
  useEffect(() => {
    const handler = (e: MouseEvent) => setMousePos({ x: e.clientX, y: e.clientY });
    document.addEventListener("mousemove", handler);

    // Get initial mouse position from Rust
    invoke<[number, number] | null>("get_mouse_position").then((pos) => {
      if (pos) setMousePos({ x: pos[0], y: pos[1] });
    });

    return () => document.removeEventListener("mousemove", handler);
  }, []);

  const doCapture = useCallback(async () => {
    if (!selectionRect) return;

    const region = {
      x: Math.round(selectionRect.x),
      y: Math.round(selectionRect.y),
      width: Math.round(selectionRect.w),
      height: Math.round(selectionRect.h),
    };

    await invoke("set_region", { region });

    if (mode === "image") {
      // Hide selection before screenshot
      setShowToolbar(false);
      setSelectionRect(null);
      await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      // Pass scale to save_screenshot
      await invoke("save_screenshot", { scale: outputScale });
    } else if (mode === "gif") {
      // GIF mode: just start recording, config is set in editor later
      await invoke("start_recording");
    }

    await closeWindow();
  }, [selectionRect, mode, outputScale, closeWindow]);

  // Mouse events
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if ((e.target as HTMLElement).closest("#toolbar")) return;

    setShowToolbar(false);
    setSelectionRect(null);
    setShowHint(false);

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
    (e: React.MouseEvent) => {
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
        setShowHint(true);
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

  return (
    <div
      className={`selector-container ${showCrosshair ? "hide-cursor" : ""}`}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
    >
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
            className="toolbar-btn"
            disabled
            style={{ opacity: 0.4, cursor: "not-allowed" }}
            title="Record Video (V)"
          >
            🎥
          </button>
          <div className="toolbar-divider" />
          {mode === "image" && (
            <select
              className="toolbar-select"
              value={outputScale}
              onChange={(e) => setOutputScale(Number(e.target.value) as OutputScale)}
              title="Output Scale"
            >
              <option value={1}>1x</option>
              <option value={0.75}>75%</option>
              <option value={0.5}>50%</option>
              <option value={0.25}>25%</option>
            </select>
          )}
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
    </div>
  );
}
