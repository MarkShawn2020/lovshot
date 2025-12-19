import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Accordion, AccordionItem, AccordionTrigger, AccordionContent } from "./components/Accordion";

interface ShortcutConfig {
  modifiers: string[];
  key: string;
  enabled: boolean;
}

interface AppConfig {
  version: string;
  shortcuts: Record<string, ShortcutConfig[]>;
  developer_mode: boolean;
  autostart_enabled: boolean;
  scroll_capture_enabled: boolean;
}

type EditingState = {
  action: string;
  index: number; // -1 means adding new
} | null;

const ACTION_LABELS: Record<string, string> = {
  screenshot_static: "Screenshot (Static)",
  screenshot: "Screenshot (Dynamic)",
  gif: "Record GIF",
  stop_recording: "Stop GIF (extra)",
  video: "Record Video",
  scroll: "Scroll Capture",
  stop_scroll: "Stop Scroll (extra)",
  show_main: "Open Main Panel",
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
  const key = cfg.key === "Escape" ? "Esc" : cfg.key;
  return [...mods, key].join("");
}

function parseKeyboardEvent(e: KeyboardEvent): { modifiers: string[]; key: string } | null {
  // Ignore pure modifier keys (but allow Escape)
  if (["Control", "Alt", "Shift", "Meta", "CapsLock", "Tab"].includes(e.key)) {
    return null;
  }

  const modifiers: string[] = [];
  if (e.altKey) modifiers.push("Alt");
  if (e.ctrlKey) modifiers.push("Ctrl");
  if (e.shiftKey) modifiers.push("Shift");
  if (e.metaKey) modifiers.push("Cmd");

  // Allow Escape without modifiers
  if (e.key === "Escape") {
    return { modifiers, key: "Escape" };
  }

  // Must have at least one modifier for other keys
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
  const [editing, setEditing] = useState<EditingState>(null);
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

  const startEditing = useCallback((action: string, index: number) => {
    setEditing({ action, index });
    setPendingShortcut(null);
    setError(null);
    // Focus the container to receive keyboard events
    setTimeout(() => containerRef.current?.focus(), 100);
  }, []);

  const handleSave = useCallback(async () => {
    if (!editing || !pendingShortcut || !config) return;

    const newShortcut: ShortcutConfig = {
      modifiers: pendingShortcut.modifiers,
      key: pendingShortcut.key,
      enabled: true,
    };

    try {
      let newConfig: AppConfig;
      if (editing.index === -1) {
        // Adding new shortcut
        newConfig = await invoke<AppConfig>("add_shortcut", {
          action: editing.action,
          shortcut: newShortcut,
        });
      } else {
        // Editing existing shortcut - replace all with updated list
        const shortcuts = [...(config.shortcuts[editing.action] || [])];
        shortcuts[editing.index] = newShortcut;
        newConfig = await invoke<AppConfig>("save_shortcut", {
          action: editing.action,
          shortcuts,
        });
      }
      setConfig(newConfig);
      setEditing(null);
      setPendingShortcut(null);
      setError(null);
      setDebugInfo("");
    } catch (e) {
      setError(String(e));
    }
  }, [editing, pendingShortcut, config]);

  const handleRemove = useCallback(async (action: string, index: number) => {
    try {
      const newConfig = await invoke<AppConfig>("remove_shortcut", {
        action,
        index,
      });
      setConfig(newConfig);
    } catch (e) {
      setError(String(e));
    }
  }, []);

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


  const handleToggleAutostart = useCallback(async () => {
    if (!config) return;
    try {
      const newConfig = await invoke<AppConfig>("set_autostart_enabled", {
        enabled: !config.autostart_enabled,
      });
      setConfig(newConfig);
    } catch (e) {
      setError(String(e));
    }
  }, [config]);

  const handleToggleScrollCapture = useCallback(async () => {
    if (!config) return;
    try {
      const newConfig = await invoke<AppConfig>("set_scroll_capture_enabled", {
        enabled: !config.scroll_capture_enabled,
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

  const actions = ["screenshot_static", "screenshot", "gif", "stop_recording", "scroll", "stop_scroll", "video", "show_main"];

  return (
    <div className="settings-container" ref={containerRef} tabIndex={-1}>
      <Accordion type="multiple" defaultValue={["shortcuts", "general", "advanced"]}>
        <AccordionItem value="shortcuts">
          <AccordionTrigger>Shortcuts</AccordionTrigger>
          <AccordionContent>
            <div className="settings-card">
              {actions.map((action, actionIndex) => {
                const shortcuts = config.shortcuts[action] || [];
                const isEditingThisAction = editing?.action === action;
                const isAdding = isEditingThisAction && editing?.index === -1;
                const isDisabled = (action === "scroll" || action === "stop_scroll") && !config.scroll_capture_enabled;

                return (
                  <div
                    key={action}
                    className={`setting-row ${actionIndex < actions.length - 1 ? "has-border" : ""} ${isDisabled ? "disabled" : ""}`}
                    data-tooltip={isDisabled ? "Enable Scroll Capture (Preview) in Advanced settings first" : undefined}
                  >
                    <span className="setting-label">{ACTION_LABELS[action]}</span>
                    <div className="setting-control">
                      {shortcuts.map((cfg, idx) => {
                        const isEditingThis = isEditingThisAction && editing?.index === idx;

                        if (isEditingThis) {
                          const pendingDisplay = pendingShortcut
                            ? formatShortcut({ modifiers: pendingShortcut.modifiers, key: pendingShortcut.key, enabled: true })
                            : null;
                          return (
                            <span key={idx} className={`shortcut-key ${pendingDisplay ? "captured" : "recording"}`}>
                              {pendingDisplay || "Press..."}
                            </span>
                          );
                        }

                        return (
                          <span
                            key={idx}
                            className={`shortcut-key ${isDisabled ? "" : "clickable"}`}
                            onClick={isDisabled ? undefined : () => startEditing(action, idx)}
                            title={isDisabled ? undefined : "Click to edit"}
                          >
                            {formatShortcut(cfg)}
                            {shortcuts.length > 1 && !isDisabled && (
                              <span
                                className="shortcut-remove"
                                onClick={(e) => { e.stopPropagation(); handleRemove(action, idx); }}
                                title="Remove"
                              >
                                ×
                              </span>
                            )}
                          </span>
                        );
                      })}

                      {isAdding && (
                        <span className={`shortcut-key ${pendingShortcut ? "captured" : "recording"}`}>
                          {pendingShortcut
                            ? formatShortcut({ modifiers: pendingShortcut.modifiers, key: pendingShortcut.key, enabled: true })
                            : "Press..."}
                        </span>
                      )}

                      {shortcuts.length === 0 && !isAdding && (
                        <span className="shortcut-key text-muted">Not set</span>
                      )}

                      {isEditingThisAction ? (
                        <>
                          <button className="btn-small" onClick={handleSave} disabled={!pendingShortcut}>
                            Save
                          </button>
                          <button className="btn-small btn-secondary" onClick={handleCancel}>
                            Cancel
                          </button>
                        </>
                      ) : (
                        <button
                          className="btn-icon"
                          onClick={() => startEditing(action, -1)}
                          title={isDisabled ? undefined : "Add shortcut"}
                          disabled={isDisabled}
                        >
                          +
                        </button>
                      )}
                    </div>
                  </div>
                );
              })}
            </div>
            {debugInfo && <div className="debug-info">{debugInfo}</div>}
            {error && <div className="error-message">{error}</div>}
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="general">
          <AccordionTrigger>General</AccordionTrigger>
          <AccordionContent>
            <div className="settings-card">
              <div className="setting-row">
                <span className="setting-label">Launch at Login</span>
                <button
                  role="switch"
                  aria-checked={config.autostart_enabled}
                  className={`switch ${config.autostart_enabled ? "switch-on" : ""}`}
                  onClick={handleToggleAutostart}
                >
                  <span className="switch-thumb" />
                </button>
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>

        <AccordionItem value="advanced">
          <AccordionTrigger>Advanced</AccordionTrigger>
          <AccordionContent>
            <div className="settings-card">
              <div className="setting-row">
                <span className="setting-label">Scroll Capture (Preview)</span>
                <button
                  role="switch"
                  aria-checked={config.scroll_capture_enabled}
                  className={`switch ${config.scroll_capture_enabled ? "switch-on" : ""}`}
                  onClick={handleToggleScrollCapture}
                >
                  <span className="switch-thumb" />
                </button>
              </div>
            </div>
          </AccordionContent>
        </AccordionItem>
      </Accordion>

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
