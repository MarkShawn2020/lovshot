import { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

type AppMode = "idle" | "recording" | "editing" | "exporting";

interface RecordingState {
  is_recording: boolean;
  frame_count: number;
}

interface RecordingInfo {
  frame_count: number;
  width: number;
  height: number;
  fps: number;
  duration_ms: number;
  has_frames: boolean;
}

interface ExportConfig {
  start_frame: number;
  end_frame: number;
  output_scale: number;
  target_fps: number;
  loop_mode: string;
}

interface SizeEstimate {
  frame_count: number;
  output_width: number;
  output_height: number;
  estimated_bytes: number;
  formatted: string;
}

interface SaveResult {
  success: boolean;
  path: string | null;
  error: string | null;
}

interface ExportProgress {
  current: number;
  total: number;
  stage: string;
}

interface ResolutionPreset {
  label: string;
  height: number;
  scale: number;
}

function App() {
  const [mode, setMode] = useState<AppMode>("idle");
  const [frameCount, setFrameCount] = useState(0);
  const [savedPath, setSavedPath] = useState("");

  // 编辑器状态
  const [recordingInfo, setRecordingInfo] = useState<RecordingInfo | null>(null);
  const [exportConfig, setExportConfig] = useState<ExportConfig>({
    start_frame: 0,
    end_frame: 0,
    output_scale: 1,
    target_fps: 10,
    loop_mode: "infinite",
  });
  const [sizeEstimate, setSizeEstimate] = useState<SizeEstimate | null>(null);

  // Filmstrip
  const [filmstrip, setFilmstrip] = useState<string[]>([]);

  // 导出进度
  const [exportProgress, setExportProgress] = useState<ExportProgress | null>(null);

  // Filmstrip 拖动状态
  const filmstripRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState<"start" | "end" | null>(null);

  // 计算可用的分辨率预设
  const resolutionPresets = useMemo<ResolutionPreset[]>(() => {
    if (!recordingInfo) return [];

    const { width, height } = recordingInfo;
    const presets: ResolutionPreset[] = [
      { label: `原始 (${width}×${height})`, height, scale: 1 },
    ];

    // 常见分辨率高度
    const standardHeights = [
      { h: 1080, label: "1080p" },
      { h: 720, label: "720p" },
      { h: 480, label: "480p" },
      { h: 360, label: "360p" },
      { h: 240, label: "240p" },
    ];

    for (const { h, label } of standardHeights) {
      if (h < height) {
        const scale = h / height;
        const scaledWidth = Math.round(width * scale);
        presets.push({
          label: `${label} (${scaledWidth}×${h})`,
          height: h,
          scale,
        });
      }
    }

    return presets;
  }, [recordingInfo]);

  // 获取体积预估
  const updateSizeEstimate = useCallback(async (config: ExportConfig) => {
    try {
      const estimate = await invoke<SizeEstimate>("estimate_export_size", { config });
      setSizeEstimate(estimate);
    } catch (e) {
      console.error("估算体积失败:", e);
    }
  }, []);

  // 监听事件
  useEffect(() => {
    const unlistenRecording = listen<RecordingState>("recording-state", (event) => {
      if (event.payload.is_recording) {
        setMode("recording");
        setFrameCount(event.payload.frame_count);
      }
    });

    const unlistenStopped = listen<{ frame_count: number }>("recording-stopped", async () => {
      try {
        const info = await invoke<RecordingInfo>("get_recording_info");
        setRecordingInfo(info);
        const initialConfig: ExportConfig = {
          start_frame: 0,
          end_frame: info.frame_count,
          output_scale: 1,
          target_fps: 10,
          loop_mode: "infinite",
        };
        setExportConfig(initialConfig);
        setMode("editing");
        updateSizeEstimate(initialConfig);
        // 加载filmstrip缩略图
        if (info.frame_count > 0) {
          invoke<string[]>("get_filmstrip", { count: 12, thumbHeight: 40 })
            .then(setFilmstrip)
            .catch((e) => console.error("加载filmstrip失败:", e));
        }
      } catch (e) {
        console.error("获取录制信息失败:", e);
        setMode("idle");
      }
    });

    const unlistenExport = listen<SaveResult>("export-complete", (event) => {
      setMode("idle");
      setRecordingInfo(null);
      setSizeEstimate(null);
      setFilmstrip([]);
      setExportProgress(null);
      if (event.payload.success && event.payload.path) {
        setSavedPath(event.payload.path);
        setTimeout(() => setSavedPath(""), 3000);
      } else if (event.payload.error) {
        console.error("导出失败:", event.payload.error);
      }
    });

    const unlistenProgress = listen<ExportProgress>("export-progress", (event) => {
      setExportProgress(event.payload);
    });

    return () => {
      unlistenRecording.then((fn) => fn());
      unlistenStopped.then((fn) => fn());
      unlistenExport.then((fn) => fn());
      unlistenProgress.then((fn) => fn());
    };
  }, [updateSizeEstimate]);

  // 配置变化时更新预估
  useEffect(() => {
    if (mode === "editing" && recordingInfo) {
      updateSizeEstimate(exportConfig);
    }
  }, [exportConfig, mode, recordingInfo, updateSizeEstimate]);

  const handleStopRecording = async () => {
    await invoke("stop_recording");
  };

  const handleExport = async () => {
    setMode("exporting");
    try {
      await invoke("export_gif", { config: exportConfig });
    } catch (e) {
      console.error("导出失败:", e);
      setMode("editing");
    }
  };

  const handleDiscard = async () => {
    await invoke("discard_recording");
    setMode("idle");
    setRecordingInfo(null);
    setSizeEstimate(null);
    setFilmstrip([]);
  };

  // Filmstrip 拖动逻辑
  const getFrameFromX = useCallback((clientX: number): number => {
    if (!filmstripRef.current || !recordingInfo) return 0;
    const rect = filmstripRef.current.getBoundingClientRect();
    const ratio = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width));
    return Math.round(ratio * recordingInfo.frame_count);
  }, [recordingInfo]);

  const handleFilmstripMouseDown = useCallback((e: React.MouseEvent, handle: "start" | "end") => {
    e.preventDefault();
    e.stopPropagation();
    setDragging(handle);
  }, []);

  // 全局 mousemove/mouseup 监听（拖拽时）
  useEffect(() => {
    if (!dragging || !recordingInfo) return;

    const handleMouseMove = (e: MouseEvent) => {
      const frame = getFrameFromX(e.clientX);
      if (dragging === "start") {
        const newStart = Math.max(0, Math.min(frame, exportConfig.end_frame - 1));
        setExportConfig((c) => ({ ...c, start_frame: newStart }));
      } else {
        const newEnd = Math.min(recordingInfo.frame_count, Math.max(frame, exportConfig.start_frame + 1));
        setExportConfig((c) => ({ ...c, end_frame: newEnd }));
      }
    };

    const handleMouseUp = () => setDragging(null);

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [dragging, recordingInfo, exportConfig.start_frame, exportConfig.end_frame, getFrameFromX]);

  const formatDuration = (ms: number) => {
    const seconds = ms / 1000;
    return seconds.toFixed(1) + "s";
  };

  const trimmedDuration = recordingInfo
    ? ((exportConfig.end_frame - exportConfig.start_frame) / recordingInfo.fps * 1000)
    : 0;

  return (
    <main className="container">
      {mode !== "editing" && (
        <div className="header">
          <h1>Lovshot</h1>
          <span className="subtitle">Unified Screen Shotter</span>
        </div>
      )}

      <div className="controls">
        {mode === "idle" && (
          <p className="shortcut-hint">
            按 <kbd>⌥</kbd> + <kbd>A</kbd> 开始录制
          </p>
        )}

        {mode === "recording" && (
          <button className="btn-stop" onClick={handleStopRecording}>
            <span className="recording-dot" />
            Stop ({frameCount})
          </button>
        )}

        {mode === "editing" && recordingInfo && (
          <div className="editor">
            {/* Filmstrip 时间线 */}
            <div className="filmstrip-section">
              <div className="timeline-labels">
                <span>{formatDuration(exportConfig.start_frame / recordingInfo.fps * 1000)}</span>
                <span className="timeline-duration">{formatDuration(trimmedDuration)}</span>
                <span>{formatDuration(exportConfig.end_frame / recordingInfo.fps * 1000)}</span>
              </div>
              <div
                ref={filmstripRef}
                className="filmstrip"
              >
                {/* 缩略图条 */}
                <div className="filmstrip-frames">
                  {filmstrip.map((src, i) => (
                    <img key={i} src={src} alt="" className="filmstrip-thumb" draggable={false} />
                  ))}
                </div>
                {/* 选区遮罩 */}
                <div
                  className="filmstrip-mask filmstrip-mask-left"
                  style={{ width: `${(exportConfig.start_frame / recordingInfo.frame_count) * 100}%` }}
                />
                <div
                  className="filmstrip-mask filmstrip-mask-right"
                  style={{ width: `${((recordingInfo.frame_count - exportConfig.end_frame) / recordingInfo.frame_count) * 100}%` }}
                />
                {/* 拖动手柄 */}
                <div
                  className="filmstrip-handle filmstrip-handle-start"
                  style={{ left: `${(exportConfig.start_frame / recordingInfo.frame_count) * 100}%` }}
                  onMouseDown={(e) => handleFilmstripMouseDown(e, "start")}
                />
                <div
                  className="filmstrip-handle filmstrip-handle-end"
                  style={{ left: `${(exportConfig.end_frame / recordingInfo.frame_count) * 100}%` }}
                  onMouseDown={(e) => handleFilmstripMouseDown(e, "end")}
                />
              </div>
            </div>

            {/* 配置选项 */}
            <div className="editor-controls">
              <div className="control-row">
                <label>Resolution</label>
                <select
                  value={exportConfig.output_scale}
                  onChange={(e) => setExportConfig((c) => ({ ...c, output_scale: parseFloat(e.target.value) }))}
                >
                  {resolutionPresets.map((preset) => (
                    <option key={preset.scale} value={preset.scale}>
                      {preset.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="control-row">
                <label>FPS</label>
                <select
                  value={exportConfig.target_fps}
                  onChange={(e) => setExportConfig((c) => ({ ...c, target_fps: parseInt(e.target.value) }))}
                >
                  <option value={5}>5</option>
                  <option value={10}>10</option>
                  <option value={15}>15</option>
                  <option value={20}>20</option>
                  <option value={30}>30</option>
                </select>
              </div>

              <div className="control-row">
                <label>Loop</label>
                <select
                  value={exportConfig.loop_mode}
                  onChange={(e) => setExportConfig((c) => ({ ...c, loop_mode: e.target.value }))}
                >
                  <option value="infinite">∞ Infinite</option>
                  <option value="once">1x Once</option>
                  <option value="pingpong">↔ Ping-pong</option>
                </select>
              </div>
            </div>

            {/* 体积预估 */}
            {sizeEstimate && (
              <div className="size-estimate">
                <span>{sizeEstimate.output_width}×{sizeEstimate.output_height}</span>
                <span>{sizeEstimate.frame_count} frames</span>
                <span className="size-badge">~{sizeEstimate.formatted}</span>
              </div>
            )}

            {/* 操作按钮 */}
            <div className="editor-actions">
              <button className="btn-secondary" onClick={handleDiscard}>Discard</button>
              <button className="btn-primary" onClick={handleExport}>Export GIF</button>
            </div>
          </div>
        )}

        {mode === "exporting" && (
          <div className="exporting-status">
            <p className="saving-hint">
              Exporting GIF...
              {exportProgress && ` (${exportProgress.current}/${exportProgress.total})`}
            </p>
            {exportProgress && (
              <div className="progress-bar">
                <div
                  className="progress-fill"
                  style={{ width: `${(exportProgress.current / exportProgress.total) * 100}%` }}
                />
              </div>
            )}
          </div>
        )}
      </div>

      {savedPath && <div className="saved-toast">Saved!</div>}
    </main>
  );
}

export default App;
