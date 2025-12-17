import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface ShortcutConfig {
  modifiers: string[];
  key: string;
  enabled: boolean;
}

interface AppConfig {
  version: string;
  shortcuts: Record<string, ShortcutConfig>;
  developer_mode: boolean;
}

type EditingAction = "screenshot" | "gif" | "video" | "scroll" | null;

const ACTION_LABELS: Record<string, string> = {
  screenshot: "Screenshot",
  gif: "Record GIF",
  video: "Record Video",
  scroll: "Scroll Capture",
};

function formatShortcut(cfg: ShortcutConfig): string {
  const mods = cfg.modifiers.map((m) => {
    switch (m.toLowerCase()) {
      case "alt": return "⌥";
      case "ctrl":
      case "control": return "⌃";
      case "shift": return "⇧";
      case "cmd":
      case "command":
      case "super":
      case "meta": return "⌘";
      default: return m;
    }
  });
  return [...mods, cfg.key].join("");
}

function parseKeyboardEvent(e: KeyboardEvent): { modifiers: string[]; key: string } | null {
  // Ignore pure modifier keys
  if (["Control", "Alt", "Shift", "Meta", "CapsLock", "Tab", "Escape"].includes(e.key)) {
    return null;
  }

  const modifiers: string[] = [];
  if (e.altKey) modifiers.push("Alt");
  if (e.ctrlKey) modifiers.push("Ctrl");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.metaKey) modifiers.push("Cmd");

  // Must have at least one modifier
  if (modifiers.length === 0) {
    return null;
  }

  // Get key (A-Z, 0-9 only)
  let key = e.code;
  if (key.startsWith("Key")) {
    key = key.slice(3); // "KeyA" -> "A"
  } else if (key.startsWith("Digit")) {
    key = key.slice(5); // "Digit1" -> "1"
  } else {
    return null;
  }

  return { modifiers, key };
}

export default function Settings() {
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [editing, setEditing] = useState<EditingAction>(null);
  const [pendingShortcut, setPendingShortcut] = useState<{ modifiers: string[]; key: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [debugInfo, setDebugInfo] = useState<string>("");
  const containerRef = useRef<HTMLDivElement>(null);

  // Load config on mount
  useEffect(() => {
    invoke<AppConfig>("get_shortcuts_config").then(setConfig);
  }, []);

  // Global keyboard listener when editing
  useEffect(() => {
    if (!editing) {
      setPendingShortcut(null);
      return;
    }

    // Pause global shortcuts
    invoke("pause_shortcuts").then(() => {
      setDebugInfo("Shortcuts paused, listening for keys...");
    });

    const handleKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();

      setDebugInfo(`Key: ${e.key}, Code: ${e.code}, Alt: ${e.altKey}, Ctrl: ${e.ctrlKey}, Meta: ${e.metaKey}`);

      const parsed = parseKeyboardEvent(e);
      if (parsed) {
        setPendingShortcut(parsed);
        setDebugInfo(`Captured: ${parsed.modifiers.join("+")}+${parsed.key}`);
      }
    };

    // Use capture phase to get events before anything else
    document.addEventListener("keydown", handleKeyDown, true);

    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      invoke("resume_shortcuts");
    };
  }, [editing]);

  const startEditing = useCallback((action: EditingAction) => {
    setEditing(action);
    setPendingShortcut(null);
    setError(null);
    // Focus the container to receive keyboard events
    setTimeout(() => containerRef.current?.focus(), 100);
  }, []);

  const handleSave = useCallback(async () => {
    if (!editing || !pendingShortcut) return;

    const shortcutStr = [...pendingShortcut.modifiers, pendingShortcut.key].join("+");

    try {
      const newConfig = await invoke<AppConfig>("save_shortcut", {
        action: editing,
        shortcutStr,
      });
      setConfig(newConfig);
      setEditing(null);
      setPendingShortcut(null);
      setError(null);
      setDebugInfo("");
    } catch (e) {
      setError(String(e));
    }
  }, [editing, pendingShortcut]);

  const handleCancel = useCallback(() => {
    setEditing(null);
    setPendingShortcut(null);
    setError(null);
    setDebugInfo("");
  }, []);

  const handleReset = useCallback(async () => {
    try {
      const newConfig = await invoke<AppConfig>("reset_shortcuts_to_default");
      setConfig(newConfig);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const handleToggleDeveloperMode = useCallback(async () => {
    if (!config) return;
    try {
      const newConfig = await invoke<AppConfig>("set_developer_mode", {
        enabled: !config.developer_mode,
      });
      setConfig(newConfig);
    } catch (e) {
      setError(String(e));
    }
  }, [config]);

  const handleClose = useCallback(async () => {
    await getCurrentWindow().close();
  }, []);

  if (!config) {
    return <div className="settings-container">Loading...</div>;
  }

  return (
    <div className="settings-container" ref={containerRef} tabIndex={-1}>
      <section className="settings-section">
        <h2 className="section-title">Shortcuts</h2>
        <div className="settings-card">
          {(config.developer_mode
            ? (["screenshot", "gif", "scroll", "video"] as const)
            : (["screenshot", "gif", "video"] as const)
          ).map((action, index, arr) => {
            const cfg = config.shortcuts[action];
            const isEditing = editing === action;
            const displayValue = cfg ? formatShortcut(cfg) : "Not set";
            const pendingDisplay = pendingShortcut
              ? formatShortcut({ modifiers: pendingShortcut.modifiers, key: pendingShortcut.key, enabled: true })
              : null;

            return (
              <div key={action} className={`setting-row ${isEditing ? "editing" : ""} ${index < arr.length - 1 ? "has-border" : ""}`}>
                <span className="setting-label">{ACTION_LABELS[action]}</span>
                <div className="setting-control">
                  {isEditing ? (
                    <>
                      <span className={`shortcut-key ${pendingDisplay ? "captured" : "recording"}`}>
                        {pendingDisplay || "Press shortcut..."}
                      </span>
                      <button className="btn-small" onClick={handleSave} disabled={!pendingShortcut}>
                        Save
                      </button>
                      <button className="btn-small btn-secondary" onClick={handleCancel}>
                        Cancel
                      </button>
                    </>
                  ) : (
                    <>
                      <span className="shortcut-key">{displayValue}</span>
                      <button className="btn-small" onClick={() => startEditing(action)}>
                        Edit
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })}
        </div>
        {debugInfo && <div className="debug-info">{debugInfo}</div>}
        {error && <div className="error-message">{error}</div>}
      </section>

      <section className="settings-section">
        <h2 className="section-title">Advanced</h2>
        <div className="settings-card">
          <div className="setting-row">
            <span className="setting-label">Developer Mode</span>
            <button
              role="switch"
              aria-checked={config.developer_mode}
              className={`switch ${config.developer_mode ? "switch-on" : ""}`}
              onClick={handleToggleDeveloperMode}
            >
              <span className="switch-thumb" />
            </button>
          </div>
        </div>
      </section>

      <div className="settings-actions">
        <button className="btn-secondary" onClick={handleReset}>
          Reset to Defaults
        </button>
        <button className="btn-primary" onClick={handleClose}>
          Done
        </button>
      </div>
    </div>
  );
}
