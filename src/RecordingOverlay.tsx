import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./recording-overlay.css";

interface OverlayRegion {
  x: number;
  y: number;
  width: number;
  height: number;
}

export default function RecordingOverlay() {
  const [region, setRegion] = useState<OverlayRegion | null>(null);
  const [isStatic, setIsStatic] = useState(false);

  useEffect(() => {
    // Get region from window label query params or listen for it
    const params = new URLSearchParams(window.location.search);
    const x = parseInt(params.get("x") || "0");
    const y = parseInt(params.get("y") || "0");
    const w = parseInt(params.get("w") || "200");
    const h = parseInt(params.get("h") || "200");
    const staticMode = params.get("static") === "1";
    setRegion({ x, y, width: w, height: h });
    setIsStatic(staticMode);

    // Listen for recording stop to close (for GIF recording)
    const unlistenRecording = listen("recording-stopped", async () => {
      await getCurrentWindow().close();
    });

    // Listen for scroll capture stop to close (for scroll capture)
    const unlistenScroll = listen("scroll-capture-stop", async () => {
      await getCurrentWindow().close();
    });

    return () => {
      unlistenRecording.then((fn) => fn());
      unlistenScroll.then((fn) => fn());
    };
  }, []);

  if (!region) return null;

  const cornerLen = 20;
  const borderWidth = 3;
  const cornerClass = isStatic ? "corner static" : "corner";

  return (
    <div className="recording-overlay">
      {/* Top-left corner - outside */}
      <div
        className={cornerClass}
        style={{
          left: region.x - borderWidth,
          top: region.y - borderWidth,
          width: cornerLen,
          height: cornerLen,
          borderWidth: `${borderWidth}px 0 0 ${borderWidth}px`,
        }}
      />
      {/* Top-right corner - outside */}
      <div
        className={cornerClass}
        style={{
          left: region.x + region.width - cornerLen + borderWidth,
          top: region.y - borderWidth,
          width: cornerLen,
          height: cornerLen,
          borderWidth: `${borderWidth}px ${borderWidth}px 0 0`,
        }}
      />
      {/* Bottom-left corner - outside */}
      <div
        className={cornerClass}
        style={{
          left: region.x - borderWidth,
          top: region.y + region.height - cornerLen + borderWidth,
          width: cornerLen,
          height: cornerLen,
          borderWidth: `0 0 ${borderWidth}px ${borderWidth}px`,
        }}
      />
      {/* Bottom-right corner - outside */}
      <div
        className={cornerClass}
        style={{
          left: region.x + region.width - cornerLen + borderWidth,
          top: region.y + region.height - cornerLen + borderWidth,
          width: cornerLen,
          height: cornerLen,
          borderWidth: `0 ${borderWidth}px ${borderWidth}px 0`,
        }}
      />
    </div>
  );
}
