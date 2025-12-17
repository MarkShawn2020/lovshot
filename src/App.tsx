import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

interface HistoryItem {
  path: string;
  filename: string;
  file_type: "screenshot" | "gif";
  modified: number;
  thumbnail: string;
}

interface HistoryResponse {
  items: HistoryItem[];
  has_more: boolean;
}

const PAGE_SIZE = 12;

function App() {
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const loaderRef = useRef<HTMLDivElement>(null);

  const loadHistory = useCallback(async (reset = false) => {
    if (loading) return;
    setLoading(true);
    try {
      const offset = reset ? 0 : history.length;
      const res = await invoke<HistoryResponse>("get_history", { offset, limit: PAGE_SIZE });
      setHistory(prev => reset ? res.items : [...prev, ...res.items]);
      setHasMore(res.has_more);
    } catch (e) {
      console.error("加载历史记录失败:", e);
    } finally {
      setLoading(false);
    }
  }, [history.length, loading]);

  useEffect(() => {
    loadHistory(true);
  }, []);

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

  return (
    <main className="container">
      <div className="header">
        <h1>Lovshot</h1>
        <span className="subtitle">Unified Screen Shotter</span>
      </div>

      <div className="controls">
        <div className="idle-content">
          <p className="shortcut-hint">
            按 <kbd>⌥</kbd> + <kbd>A</kbd> 开始截图
          </p>
          {history.length > 0 && (
            <div className="history-section">
              <h3>历史记录</h3>
              <div className="history-grid">
                {history.map((item) => (
                  <div
                    key={item.path}
                    className="history-item"
                    onClick={() => invoke("open_file", { path: item.path })}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      invoke("reveal_in_folder", { path: item.path });
                    }}
                    title={`${item.filename}\n右键在 Finder 中显示`}
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
              {hasMore && (
                <div ref={loaderRef} className="history-loader">
                  {loading ? "加载中..." : ""}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

export default App;
