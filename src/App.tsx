import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";

interface RecordingState {
  is_recording: boolean;
  frame_count: number;
}

interface SaveResult {
  success: boolean;
  path: string | null;
  error: string | null;
}

function App() {
  const [isRecording, setIsRecording] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [frameCount, setFrameCount] = useState(0);
  const [savedPath, setSavedPath] = useState("");

  useEffect(() => {
    const unlistenRecording = listen<RecordingState>("recording-state", (event) => {
      setIsRecording(event.payload.is_recording);
      setFrameCount(event.payload.frame_count);
    });

    const unlistenSave = listen<SaveResult>("save-complete", (event) => {
      console.log("[DEBUG] save-complete 事件收到:", event.payload);
      setIsSaving(false);
      setFrameCount(0);
      if (event.payload.success && event.payload.path) {
        console.log("[DEBUG] 保存成功, 路径:", event.payload.path);
        setSavedPath(event.payload.path);
        setTimeout(() => setSavedPath(""), 3000);
      } else if (event.payload.error) {
        console.error("Save failed:", event.payload.error);
      }
    });

    return () => {
      unlistenRecording.then((fn) => fn());
      unlistenSave.then((fn) => fn());
    };
  }, []);

  const handleStopRecording = async () => {
    await invoke("stop_recording");
    setIsRecording(false);
    setIsSaving(true);
    // save_gif 立即返回，编码在后台进行，完成后通过 save-complete 事件通知
    await invoke("save_gif");
  };

  return (
    <main className="container">
      <div className="header">
        <h1>Lovshot</h1>
        <span className="subtitle">Unified Screen Shotter</span>
      </div>

      <div className="controls">
        {isRecording ? (
          <button className="btn-stop" onClick={handleStopRecording}>
            <span className="recording-dot" />
            Stop ({frameCount})
          </button>
        ) : isSaving ? (
          <p className="saving-hint">Saving GIF...</p>
        ) : (
          <p className="shortcut-hint">
            按 <kbd>⇧</kbd> + <kbd>⌥</kbd> + <kbd>A</kbd> 开始录制
          </p>
        )}
      </div>

      {savedPath && <div className="saved-toast">Saved!</div>}
    </main>
  );
}

export default App;
