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

export default function ScrollOverlay() {
  const [progress, setProgress] = useState<ScrollCaptureProgress | null>(null);
  const [isStopped, setIsStopped] = useState(false);
  const [pollingEnabled, setPollingEnabled] = useState(true);
  const isClosingRef = useRef(false); // Prevent double-close

  // Listen for instant initial preview data pushed from backend
  useEffect(() => {
    const unlisten = listen<ScrollCaptureProgress>("scroll-preview-update", (event) => {
      setProgress(event.payload);
    });
    return () => { unlisten.then(fn => fn()); };
  }, []);

  // Switch between event-driven and polling modes
  useEffect(() => {
    const unlistenStarted = listen("scroll-listener-started", () => {
      console.log("[ScrollOverlay] Scroll listener active, disable polling");
      setPollingEnabled(false);
    });
    const unlistenFailed = listen("scroll-listener-failed", () => {
      console.warn("[ScrollOverlay] Scroll listener failed, fallback to polling");
      setPollingEnabled(true);
    });

    return () => {
      unlistenStarted.then(fn => fn());
      unlistenFailed.then(fn => fn());
    };
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

    if (!pollingEnabled) return;

    const intervalId = setInterval(pollCapture, POLL_INTERVAL);
    return () => clearInterval(intervalId);
  }, [isStopped, progress, pollingEnabled]);

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

  const handleStop = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    await invoke("stop_scroll_capture");
    setIsStopped(true);
  };

  const handleFinish = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const filePath = await save({
        defaultPath: `scroll_${timestamp}.png`,
        filters: [{ name: "PNG Image", extensions: ["png"] }],
      });

      if (!filePath) return;

      await getCurrentWindow().hide();
      await invoke<string>("finish_scroll_capture", { path: filePath, crop: null });
      await getCurrentWindow().destroy();
    } catch (e) {
      console.error("[ScrollOverlay] handleFinish error:", e);
    }
  };

  const handleCopy = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    try {
      await invoke("copy_scroll_to_clipboard", { crop: null });
    } catch (e) {
      console.error("[ScrollOverlay] copy error:", e);
    }
  };

  // handleCancel uses the same closeAndCancel to prevent double-close
  const handleCancel = (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();
    closeAndCancel();
  };

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

      <button className="btn-close" onMouseDown={handleCancel} title="Close">
        <svg width="12" height="12" viewBox="0 0 14 14" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
          <path d="M1 1L13 13M13 1L1 13" />
        </svg>
      </button>

      <div className="scroll-overlay-header" data-tauri-drag-region>
        <span data-tauri-drag-region>Lovshot Preview</span>
      </div>

      <div className="scroll-overlay-preview">
        {progress && (
          <div className="preview-wrapper">
            <img src={progress.preview_base64} alt="" draggable={false} />
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
          <button className="btn-stop" onMouseDown={handleStop}>Stop</button>
        ) : (
          <>
            <button className="btn-copy" onMouseDown={handleCopy}>Copy</button>
            <button className="btn-save" onMouseDown={handleFinish}>Save</button>
          </>
        )}
      </div>
    </div>
  );
}
