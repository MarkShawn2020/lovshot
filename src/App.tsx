import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import "./App.css";

interface HistoryItem {
  path: string;
  filename: string;
  file_type: "screenshot" | "gif";
  modified: number;
  size: number;
  thumbnail: string;
}

interface HistoryResponse {
  items: HistoryItem[];
  has_more: boolean;
  total: number;
}

interface Stats {
  total_count: number;
  screenshot_count: number;
  gif_count: number;
  total_size: number;
  today_count: number;
  week_count: number;
}

type FilterType = "all" | "screenshot" | "gif";

const PAGE_SIZE = 12;

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(timestamp: number): string {
  const date = new Date(timestamp * 1000);
  const now = new Date();
  const diff = now.getTime() - date.getTime();

  if (diff < 60000) return "刚刚";
  if (diff < 3600000) return `${Math.floor(diff / 60000)} 分钟前`;
  if (diff < 86400000) return `${Math.floor(diff / 3600000)} 小时前`;
  if (diff < 604800000) return `${Math.floor(diff / 86400000)} 天前`;

  return date.toLocaleDateString("zh-CN", { month: "short", day: "numeric" });
}

function App() {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [stats, setStats] = useState<Stats | null>(null);
  const [filter, setFilter] = useState<FilterType>("all");
  const [selected, setSelected] = useState<HistoryItem | null>(null);
  const [isWideScreen, setIsWideScreen] = useState(window.innerWidth >= 700);
  const loaderRef = useRef<HTMLDivElement>(null);

  // Track screen width for gallery layout
  useEffect(() => {
    const handleResize = () => setIsWideScreen(window.innerWidth >= 700);
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  const loadStats = useCallback(async () => {
    try {
      const res = await invoke<Stats>("get_stats");
      setStats(res);
    } catch (e) {
      console.error("加载统计失败:", e);
    }
  }, []);

  const loadHistory = useCallback(async (reset = false) => {
    if (loading) return;
    setLoading(true);
    try {
      const offset = reset ? 0 : history.length;
      const filterType = filter === "all" ? null : filter;
      const res = await invoke<HistoryResponse>("get_history", {
        offset,
        limit: PAGE_SIZE,
        filterType,
      });
      setHistory(prev => reset ? res.items : [...prev, ...res.items]);
      setHasMore(res.has_more);
      // Auto-select first item on wide screen
      if (reset && res.items.length > 0 && isWideScreen) {
        setSelected(res.items[0]);
      }
    } catch (e) {
      console.error("加载历史记录失败:", e);
    } finally {
      setLoading(false);
    }
  }, [history.length, loading, filter, isWideScreen]);

  useEffect(() => {
    loadStats();
    loadHistory(true);
  }, []);

  useEffect(() => {
    setSelected(null);
    loadHistory(true);
  }, [filter]);

  useEffect(() => {
    const loader = loaderRef.current;
    if (!loader || !hasMore) return;

    const observer = new IntersectionObserver(
      entries => {
        if (entries[0].isIntersecting && !loading) {
          loadHistory();
        }
      },
      { threshold: 0.1 }
    );

    observer.observe(loader);
    return () => observer.disconnect();
  }, [hasMore, loading, loadHistory]);

  const handleItemClick = (item: HistoryItem) => {
    if (isWideScreen) {
      setSelected(item);
    } else {
      invoke("open_file", { path: item.path });
    }
  };

  const handleOpenExternal = () => {
    if (selected) {
      invoke("open_file", { path: selected.path });
    }
  };

  const handleRevealInFinder = () => {
    if (selected) {
      invoke("reveal_in_folder", { path: selected.path });
    }
  };

  return (
    <main className={`dashboard ${isWideScreen ? "gallery-mode" : ""}`}>
      {/* Left Panel - List */}
      <div className="gallery-list">
        <div className="dashboard-header">
          <div className="logo-section">
            <h1>Lovshot</h1>
            <span className="subtitle">Unified Screen Shotter</span>
          </div>
          <p className="shortcut-hint">
            <kbd>⌥</kbd><kbd>A</kbd> 截图 · <kbd>⌥</kbd><kbd>G</kbd> GIF
          </p>
        </div>

        {stats && (
          <div className="stats-grid">
            <div className="stat-card">
              <span className="stat-value">{stats.total_count}</span>
              <span className="stat-label">总数</span>
            </div>
            <div className="stat-card">
              <span className="stat-value">{stats.screenshot_count}</span>
              <span className="stat-label">截图</span>
            </div>
            <div className="stat-card">
              <span className="stat-value">{stats.gif_count}</span>
              <span className="stat-label">GIF</span>
            </div>
            <div className="stat-card">
              <span className="stat-value">{formatSize(stats.total_size)}</span>
              <span className="stat-label">存储</span>
            </div>
          </div>
        )}

        <div className="history-section">
          <div className="history-header">
            <div className="filter-tabs">
              <button
                className={`filter-tab ${filter === "all" ? "active" : ""}`}
                onClick={() => setFilter("all")}
              >
                全部
              </button>
              <button
                className={`filter-tab ${filter === "screenshot" ? "active" : ""}`}
                onClick={() => setFilter("screenshot")}
              >
                截图
              </button>
              <button
                className={`filter-tab ${filter === "gif" ? "active" : ""}`}
                onClick={() => setFilter("gif")}
              >
                GIF
              </button>
            </div>
          </div>

          <div className="history-scroll">
            {history.length === 0 && !loading ? (
              <div className="empty-state">
                <span className="empty-icon">📷</span>
                <p>暂无{filter === "gif" ? "GIF" : filter === "screenshot" ? "截图" : "记录"}</p>
                <p className="empty-hint">使用快捷键开始截图吧</p>
              </div>
            ) : (
              <div className="history-grid">
                {history.map((item) => (
                  <div
                    key={item.path}
                    className={`history-item ${selected?.path === item.path ? "selected" : ""}`}
                    onClick={() => handleItemClick(item)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      invoke("reveal_in_folder", { path: item.path });
                    }}
                    title={isWideScreen ? item.filename : `${item.filename}\n${formatSize(item.size)}\n右键在 Finder 中显示`}
                  >
                    {item.thumbnail ? (
                      <img src={item.thumbnail} alt={item.filename} className="history-thumb" />
                    ) : (
                      <div className="history-thumb-placeholder" />
                    )}
                    <span className={`history-badge history-badge-${item.file_type}`}>
                      {item.file_type === "gif" ? "GIF" : "IMG"}
                    </span>
                  </div>
                ))}
              </div>
            )}

            {hasMore && (
              <div ref={loaderRef} className="history-loader">
                {loading ? "加载中..." : ""}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Right Panel - Preview (gallery mode only) */}
      {isWideScreen && (
        <div className="gallery-preview">
          {selected ? (
            <>
              <div className="preview-image-container">
                <img
                  src={convertFileSrc(selected.path)}
                  alt={selected.filename}
                  className="preview-full-image"
                />
              </div>
              <div className="preview-info">
                <h3 className="preview-filename">{selected.filename}</h3>
                <div className="preview-meta">
                  <span>{formatSize(selected.size)}</span>
                  <span>·</span>
                  <span>{formatDate(selected.modified)}</span>
                  <span>·</span>
                  <span className={`preview-type preview-type-${selected.file_type}`}>
                    {selected.file_type === "gif" ? "GIF" : "Screenshot"}
                  </span>
                </div>
                <div className="preview-actions">
                  <button className="btn-action" onClick={handleOpenExternal}>
                    打开
                  </button>
                  <button className="btn-action btn-secondary" onClick={handleRevealInFinder}>
                    在 Finder 中显示
                  </button>
                </div>
              </div>
            </>
          ) : (
            <div className="preview-empty">
              <span className="preview-empty-icon">👆</span>
              <p>选择一张图片预览</p>
            </div>
          )}
        </div>
      )}
    </main>
  );
}

export default App;
