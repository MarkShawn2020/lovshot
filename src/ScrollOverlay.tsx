import { useState, useEffect, useRef, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";

import "./scroll-overlay.css";

interface ScrollCaptureProgress {
  frame_count: number;
  total_height: number;
  preview_base64: string;
}

interface CropEdges {
  top: number;
  bottom: number;
  left: number;
  right: number;
}

type Edge = "top" | "bottom" | "left" | "right";

export default function ScrollOverlay() {
  const [progress, setProgress] = useState<ScrollCaptureProgress | null>(null);
  const [isStopped, setIsStopped] = useState(false);
  const [crop, setCrop] = useState<CropEdges>({ top: 0, bottom: 0, left: 0, right: 0 });
  const containerRef = useRef<HTMLDivElement>(null);
  const draggingRef = useRef<Edge | null>(null);
  const isClosingRef = useRef(false); // Prevent double-close

  // Listen for instant initial preview data pushed from backend
  useEffect(() => {
    const unlisten = listen<ScrollCaptureProgress>("scroll-preview-update", (event) => {
      setProgress(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Poll for scroll changes
  useEffect(() => {
    if (isStopped) return;

    let isCapturing = false;
    const POLL_INTERVAL = 200;

    const pollCapture = async () => {
      if (isCapturing) return;
      isCapturing = true;
      try {
        const result = await invoke<ScrollCaptureProgress | null>("capture_scroll_frame_auto");
        if (result) setProgress(result);
      } catch {
        // ignore
      } finally {
        isCapturing = false;
      }
    };

    // Fallback: fetch initial data if not received via event
    if (!progress) {
      invoke<ScrollCaptureProgress>("get_scroll_preview")
        .then(setProgress)
        .catch(() => {});
    }

    const intervalId = setInterval(pollCapture, POLL_INTERVAL);
    return () => clearInterval(intervalId);
  }, [isStopped, progress]);

  // Centralized close function to prevent double-close
  const closeAndCancel = useCallback(async () => {
    if (isClosingRef.current) return;
    isClosingRef.current = true;
    console.log("[ScrollOverlay] Closing...");
    try {
      await invoke("cancel_scroll_capture");
    } catch { /* ignore */ }
    try {
      await getCurrentWindow().destroy();
    } catch { /* ignore */ }
  }, []);

  // Listen for global shortcut event (backend sends this when ESC is pressed globally)
  useEffect(() => {
    const unlisten = listen("scroll-capture-stop", closeAndCancel);
    return () => { unlisten.then(fn => fn()); };
  }, [closeAndCancel]);

  // LOCAL ESC key listener as fallback (for when global shortcut doesn't work)
  // This is needed because scroll-overlay is a non-activating panel
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        console.log("[ScrollOverlay] Local ESC detected");
        closeAndCancel();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [closeAndCancel]);

  const handleMouseMove = useCallback((e: MouseEvent) => {
    const edge = draggingRef.current;
    if (!edge || !containerRef.current) return;

    const rect = containerRef.current.getBoundingClientRect();

    if (edge === "top") {
      const pct = Math.max(0, Math.min(90, ((e.clientY - rect.top) / rect.height) * 100));
      setCrop(c => ({ ...c, top: pct }));
    } else if (edge === "bottom") {
      const pct = Math.max(0, Math.min(90, ((rect.bottom - e.clientY) / rect.height) * 100));
      setCrop(c => ({ ...c, bottom: pct }));
    } else if (edge === "left") {
      const pct = Math.max(0, Math.min(90, ((e.clientX - rect.left) / rect.width) * 100));
      setCrop(c => ({ ...c, left: pct }));
    } else if (edge === "right") {
      const pct = Math.max(0, Math.min(90, ((rect.right - e.clientX) / rect.width) * 100));
      setCrop(c => ({ ...c, right: pct }));
    }
  }, []);

  const handleMouseUp = useCallback(() => {
    draggingRef.current = null;
    document.removeEventListener("mousemove", handleMouseMove);
    document.removeEventListener("mouseup", handleMouseUp);
  }, [handleMouseMove]);

  const startDrag = (edge: Edge) => (e: React.MouseEvent) => {
    e.preventDefault();
    draggingRef.current = edge;
    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  };

  const handleStop = async () => {
    await invoke("stop_scroll_capture");
    setIsStopped(true);
  };

  const getCropParam = (): CropEdges | null => {
    if (crop.top === 0 && crop.bottom === 0 && crop.left === 0 && crop.right === 0) return null;
    return crop;
  };

  const handleFinish = async () => {
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const filePath = await save({
        defaultPath: `scroll_${timestamp}.png`,
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });

      if (!filePath) return;

      await getCurrentWindow().hide();
      await invoke<string>("finish_scroll_capture", { path: filePath, crop: getCropParam() });
      await getCurrentWindow().destroy();
    } catch (e) {
      console.error("[ScrollOverlay] handleFinish error:", e);
    }
  };

  const handleCopy = async () => {
    try {
      await invoke("copy_scroll_to_clipboard", { crop: getCropParam() });
    } catch (e) {
      console.error("[ScrollOverlay] copy error:", e);
    }
  };

  // handleCancel uses the same closeAndCancel to prevent double-close
  const handleCancel = closeAndCancel;

  const startResize = (direction: "North" | "South" | "East" | "West" | "NorthEast" | "NorthWest" | "SouthEast" | "SouthWest") => async (e: React.MouseEvent) => {
    e.preventDefault();
    await getCurrentWindow().startResizeDragging(direction);
  };

  return (
    <div className="scroll-overlay-container">
      {/* Window resize handles */}
      <div className="resize-handle resize-n" onMouseDown={startResize("North")} />
      <div className="resize-handle resize-s" onMouseDown={startResize("South")} />
      <div className="resize-handle resize-e" onMouseDown={startResize("East")} />
      <div className="resize-handle resize-w" onMouseDown={startResize("West")} />
      <div className="resize-handle resize-nw" onMouseDown={startResize("NorthWest")} />
      <div className="resize-handle resize-ne" onMouseDown={startResize("NorthEast")} />
      <div className="resize-handle resize-sw" onMouseDown={startResize("SouthWest")} />
      <div className="resize-handle resize-se" onMouseDown={startResize("SouthEast")} />

      <button className="btn-close" onClick={handleCancel} title="Close">
        <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <path d="M1 1L13 13M13 1L1 13" />
        </svg>
      </button>

      <div className="scroll-overlay-header" data-tauri-drag-region>
        <span data-tauri-drag-region>Lovshot Preview</span>
      </div>

      <div className="scroll-overlay-preview">
        {progress && (
          <div className="crop-wrapper" ref={containerRef}>
            <img src={progress.preview_base64} alt="" draggable={false} />

            {isStopped && (
              <>
                {/* Crop overlays */}
                <div className="crop-overlay crop-top" style={{ height: `${crop.top}%` }} />
                <div className="crop-overlay crop-bottom" style={{ height: `${crop.bottom}%` }} />
                <div className="crop-overlay crop-left" style={{ width: `${crop.left}%`, top: `${crop.top}%`, bottom: `${crop.bottom}%` }} />
                <div className="crop-overlay crop-right" style={{ width: `${crop.right}%`, top: `${crop.top}%`, bottom: `${crop.bottom}%` }} />

                {/* Drag handles */}
                <div className="crop-handle crop-handle-top" style={{ top: `${crop.top}%` }} onMouseDown={startDrag("top")} />
                <div className="crop-handle crop-handle-bottom" style={{ bottom: `${crop.bottom}%` }} onMouseDown={startDrag("bottom")} />
                <div className="crop-handle crop-handle-left" style={{ left: `${crop.left}%` }} onMouseDown={startDrag("left")} />
                <div className="crop-handle crop-handle-right" style={{ right: `${crop.right}%` }} onMouseDown={startDrag("right")} />
              </>
            )}
          </div>
        )}
      </div>

      {progress && (
        <div className="scroll-overlay-stats">
          {progress.frame_count} frames · {progress.total_height}px
        </div>
      )}

      <div className="scroll-overlay-actions">
        {!isStopped ? (
          <button className="btn-stop" onClick={handleStop}>Stop</button>
        ) : (
          <>
            <button className="btn-copy" onClick={handleCopy}>Copy</button>
            <button className="btn-save" onClick={handleFinish}>Save</button>
          </>
        )}
      </div>
    </div>
  );
}
