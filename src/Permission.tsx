import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface PermissionStatus {
  granted: boolean;
}

export default function Permission() {
  const [checking, setChecking] = useState(false);
  const [showTroubleshooting, setShowTroubleshooting] = useState(false);

  const checkPermission = useCallback(async () => {
    setChecking(true);
    try {
      const status = await invoke<PermissionStatus>("check_screen_permission");
      if (status.granted) {
        // Permission granted, close this window
        await getCurrentWindow().close();
      }
    } catch (e) {
      console.error("检查权限失败:", e);
    } finally {
      setChecking(false);
    }
  }, []);

  // Poll for permission changes
  useEffect(() => {
    const interval = setInterval(checkPermission, 1000);
    return () => clearInterval(interval);
  }, [checkPermission]);

  const handleOpenSettings = async () => {
    try {
      await invoke("open_permission_settings");
    } catch (e) {
      console.error("打开系统偏好设置失败:", e);
    }
  };

  const handleQuit = async () => {
    await invoke("quit_app");
  };

  return (
    <div className="permission-container">
      <div className="permission-icon">
        <img src="/logo.svg" alt="Lovshot" width={64} height={64} />
      </div>

      <h1>需要屏幕录制权限</h1>

      <p className="description">
        Lovshot 需要「屏幕录制」权限才能截取屏幕内容。
        <br />
        请在系统设置中授权后重新启动应用。
      </p>

      <div className="steps">
        <div className="step">
          <span className="step-number">1</span>
          <span>点击下方按钮打开系统设置</span>
        </div>
        <div className="step">
          <span className="step-number">2</span>
          <span>找到 Lovshot 并勾选启用</span>
        </div>
        <div className="step">
          <span className="step-number">3</span>
          <span>重启 Lovshot 应用</span>
        </div>
      </div>

      <div className="actions">
        <button className="btn-primary" onClick={handleOpenSettings}>
          打开系统设置
        </button>
        <button className="btn-secondary" onClick={handleQuit}>
          退出应用
        </button>
      </div>

      <div className="troubleshooting">
        <button
          className="btn-link"
          onClick={() => setShowTroubleshooting(!showTroubleshooting)}
        >
          {showTroubleshooting ? "收起" : "授权后仍无法使用？"}
        </button>

        {showTroubleshooting && (
          <div className="troubleshooting-content">
            <p className="warning-title">常见问题解决方案：</p>
            <ol>
              <li>
                <strong>先删除旧授权</strong>：在「系统设置 → 隐私与安全性 → 屏幕录制」中，
                找到 Lovshot（或之前的授权项），点击「-」按钮删除
              </li>
              <li>
                <strong>完全退出应用</strong>：确保 Lovshot 完全退出（检查菜单栏图标）
              </li>
              <li>
                <strong>重新启动</strong>：重新打开 Lovshot，系统会重新请求授权
              </li>
              <li>
                <strong>如果截图显示为桌面壁纸</strong>：这通常是权限缓存问题，
                按上述步骤删除旧授权后重启即可
              </li>
            </ol>
            <p className="warning-note">
              注意：macOS 的权限系统有时会缓存旧的授权状态，删除后重新授权可以解决大部分问题。
            </p>
          </div>
        )}
      </div>

      {checking && <p className="checking">正在检查权限状态...</p>}
    </div>
  );
}
