import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import Masonry from "react-masonry-css";
import ExportDialog from "./ExportDialog";
import "./App.css";

interface HistoryItem {
  path: string;
  filename: string;
  file_type: "screenshot" | "gif";
  modified: number;
  size: number;
  thumbnail: string;
  description?: string; // Finder comment / caption
  isLoading?: boolean; // 占位符状态
}

interface SaveResult {
  success: boolean;
  path: string | null;
  error: string | null;
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

interface FolderInfo {
  name: string;
  path: string;
  file_count: number;
}

type FilterType = "all" | "screenshot" | "gif";

const PAGE_SIZE = 12;

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatSizeCompact(bytes: number): { value: string; unit: string } {
  if (bytes < 1024) return { value: `${bytes}`, unit: "B" };
  if (bytes < 1024 * 1024) return { value: (bytes / 1024).toFixed(1), unit: "K" };
  if (bytes < 1024 * 1024 * 1024) return { value: (bytes / (1024 * 1024)).toFixed(1), unit: "M" };
  return { value: (bytes / (1024 * 1024 * 1024)).toFixed(1), unit: "G" };
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
  const [imageDimensions, setImageDimensions] = useState<{ width: number; height: number } | null>(null);
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [isWideScreen, setIsWideScreen] = useState(window.innerWidth >= 700);
  const [sidebarWidth, setSidebarWidth] = useState(window.innerWidth >= 1000 ? 320 : 280);
  const loaderRef = useRef<HTMLDivElement>(null);
  const didDragRef = useRef(false); // Track if a valid drag occurred
  const isResizingRef = useRef(false);

  // Folder state
  const [folders, setFolders] = useState<FolderInfo[]>([]);
  const [selectedFolder, setSelectedFolder] = useState<string | null>(null); // null = root
  const [newFolderName, setNewFolderName] = useState("");
  const [isCreatingFolder, setIsCreatingFolder] = useState(false);
  const [folderMenu, setFolderMenu] = useState<{ x: number; y: number; folder: FolderInfo } | null>(null);
  const [editingFolder, setEditingFolder] = useState<{ path: string; name: string } | null>(null);
  const [exportDialog, setExportDialog] = useState<{ folderPath: string | null; folderName: string } | null>(null);

  // Context menu state
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    items: HistoryItem[];
  } | null>(null);

  // Drag selection state
  const [dragSelect, setDragSelect] = useState<{
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
  } | null>(null);

  // Clear image dimensions when selection changes
  useEffect(() => {
    setImageDimensions(null);
  }, [selected?.path]);

  // Track screen width for gallery layout
  useEffect(() => {
    const handleResize = () => setIsWideScreen(window.innerWidth >= 700);
    window.addEventListener("resize", handleResize);
    return () => window.removeEventListener("resize", handleResize);
  }, []);

  // Sidebar resize handlers
  const handleResizeStart = useCallback((e: React.MouseEvent) => {
    e.preventDefault();
    isResizingRef.current = true;
    document.body.style.cursor = "col-resize";
    document.body.style.userSelect = "none";
  }, []);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      if (!isResizingRef.current) return;
      const newWidth = Math.min(Math.max(200, e.clientX), 500);
      setSidebarWidth(newWidth);
    };
    const handleMouseUp = () => {
      if (isResizingRef.current) {
        isResizingRef.current = false;
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
      }
    };
    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, []);

  const loadStats = useCallback(async () => {
    try {
      const res = await invoke<Stats>("get_stats");
      setStats(res);
    } catch (e) {
      console.error("加载统计失败:", e);
    }
  }, []);

  const loadFolders = useCallback(async () => {
    try {
      const res = await invoke<FolderInfo[]>("get_folders");
      setFolders(res);
    } catch (e) {
      console.error("加载文件夹失败:", e);
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
        folder: selectedFolder,
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
  }, [history.length, loading, filter, isWideScreen, selectedFolder]);

  // 实时监听新截图/GIF 保存事件
  useEffect(() => {
    const unlistenScreenshot = listen<string>("screenshot-saved", (event) => {
      const path = event.payload;
      const filename = path.split("/").pop() || path;
      // 插入 loading 占位符
      const placeholder: HistoryItem = {
        path,
        filename,
        file_type: "screenshot",
        modified: Math.floor(Date.now() / 1000),
        size: 0,
        thumbnail: "",
        isLoading: true,
      };
      setHistory((prev) => [placeholder, ...prev]);
      loadStats();

      // 延迟获取完整信息（等文件写入完成）
      setTimeout(async () => {
        try {
          const res = await invoke<HistoryResponse>("get_history", {
            offset: 0,
            limit: 1,
            filterType: null,
          });
          if (res.items.length > 0) {
            setHistory((prev) =>
              prev.map((item) =>
                item.path === path ? { ...res.items[0], isLoading: false } : item
              )
            );
          }
        } catch (e) {
          console.error("获取新截图信息失败:", e);
        }
      }, 100);
    });

    const unlistenGif = listen<SaveResult>("export-complete", (event) => {
      const { success, path } = event.payload;
      if (!success || !path) return;
      const filename = path.split("/").pop() || path;
      // 插入 loading 占位符
      const placeholder: HistoryItem = {
        path,
        filename,
        file_type: "gif",
        modified: Math.floor(Date.now() / 1000),
        size: 0,
        thumbnail: "",
        isLoading: true,
      };
      setHistory((prev) => [placeholder, ...prev]);
      loadStats();
      // 延迟获取完整信息
      setTimeout(async () => {
        try {
          const res = await invoke<HistoryResponse>("get_history", {
            offset: 0,
            limit: 1,
            filterType: "gif",
          });
          if (res.items.length > 0 && res.items[0].path === path) {
            setHistory((prev) =>
              prev.map((item) =>
                item.path === path ? { ...res.items[0], isLoading: false } : item
              )
            );
          }
        } catch (e) {
          console.error("获取新 GIF 信息失败:", e);
        }
      }, 100);
    });

    // 监听 caption 实时输入，立即更新 description
    const unlistenTyping = listen<{ path: string; caption: string }>("caption-typing", (event) => {
      const { path, caption } = event.payload;
      setHistory((prev) =>
        prev.map((item) =>
          item.path === path ? { ...item, description: caption || undefined } : item
        )
      );
      setSelected((prev) =>
        prev?.path === path ? { ...prev, description: caption || undefined } : prev
      );
    });

    // 监听 caption 保存事件
    const unlistenCaption = listen<{ path: string; caption: string }>("caption-saved", (event) => {
      const { path, caption } = event.payload;
      setHistory((prev) =>
        prev.map((item) =>
          item.path === path ? { ...item, description: caption } : item
        )
      );
      setSelected((prev) =>
        prev?.path === path ? { ...prev, description: caption } : prev
      );
    });

    // 监听图片保存事件，刷新列表
    const unlistenSaved = listen<{ path: string }>("image-saved", () => {
      loadHistory(true);
      loadStats();
    });

    return () => {
      unlistenScreenshot.then((fn) => fn());
      unlistenGif.then((fn) => fn());
      unlistenTyping.then((fn) => fn());
      unlistenCaption.then((fn) => fn());
      unlistenSaved.then((fn) => fn());
    };
  }, [loadStats]);

  useEffect(() => {
    loadStats();
    loadFolders();
    loadHistory(true);
  }, []);

  // Reload history when filter or folder changes
  useEffect(() => {
    setSelected(null);
    setLoading(true);
    const filterType = filter === "all" ? null : filter;
    invoke<HistoryResponse>("get_history", {
      offset: 0,
      limit: PAGE_SIZE,
      filterType,
      folder: selectedFolder,
    }).then(res => {
      setHistory(res.items);
      setHasMore(res.has_more);
      if (res.items.length > 0 && window.innerWidth >= 700) {
        setSelected(res.items[0]);
      }
    }).catch(e => {
      console.error("加载历史记录失败:", e);
    }).finally(() => {
      setLoading(false);
    });
  }, [filter, selectedFolder]);

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

  const handleItemClick = (e: React.MouseEvent, item: HistoryItem) => {
    // Skip if we just did a drag selection
    if (didDragRef.current) {
      didDragRef.current = false;
      return;
    }
    // In multi-select mode or with Cmd/Ctrl, toggle selection
    if (selectedPaths.size > 0 || e.metaKey || e.ctrlKey) {
      setSelectedPaths((prev) => {
        const next = new Set(prev);
        if (next.has(item.path)) {
          next.delete(item.path);
        } else {
          next.add(item.path);
        }
        return next;
      });
      return;
    }
    // Normal click
    if (isWideScreen) {
      setSelected(item);
    } else {
      invoke("open_file", { path: item.path });
    }
  };

  // ESC to exit multi-select mode
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && selectedPaths.size > 0) {
        setSelectedPaths(new Set());
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [selectedPaths.size]);

  // Drag selection - start on any mousedown
  const handleDragStart = (e: React.MouseEvent) => {
    if (e.button !== 0) return;
    didDragRef.current = false;
    setDragSelect({
      startX: e.clientX,
      startY: e.clientY,
      currentX: e.clientX,
      currentY: e.clientY,
    });
  };

  // Global mouse events for drag selection
  useEffect(() => {
    if (!dragSelect) return;

    const handleMouseMove = (e: MouseEvent) => {
      setDragSelect((prev) =>
        prev ? { ...prev, currentX: e.clientX, currentY: e.clientY } : null
      );
    };

    const handleMouseUp = () => {
      setDragSelect((current) => {
        if (!current) return null;
        // Calculate selection box
        const left = Math.min(current.startX, current.currentX);
        const right = Math.max(current.startX, current.currentX);
        const top = Math.min(current.startY, current.currentY);
        const bottom = Math.max(current.startY, current.currentY);
        const boxWidth = right - left;
        const boxHeight = bottom - top;

        // Only select if dragged more than 10px
        if (boxWidth > 10 || boxHeight > 10) {
          didDragRef.current = true; // Mark that we did a valid drag
          const items = document.querySelectorAll(".history-item[data-path]");
          const newSelected = new Set(selectedPaths);
          items.forEach((el) => {
            const rect = el.getBoundingClientRect();
            const intersects =
              rect.left < right && rect.right > left && rect.top < bottom && rect.bottom > top;
            if (intersects) {
              const path = (el as HTMLElement).dataset.path;
              if (path) newSelected.add(path);
            }
          });
          setSelectedPaths(newSelected);
        }
        return null;
      });
    };

    window.addEventListener("mousemove", handleMouseMove);
    window.addEventListener("mouseup", handleMouseUp);
    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      window.removeEventListener("mouseup", handleMouseUp);
    };
  }, [dragSelect, selectedPaths]);

  // Get selection box style - only show if dragged enough
  const getSelectionBoxStyle = (): React.CSSProperties | null => {
    if (!dragSelect) return null;
    const width = Math.abs(dragSelect.currentX - dragSelect.startX);
    const height = Math.abs(dragSelect.currentY - dragSelect.startY);
    // Only show if dragged more than 10px
    if (width <= 10 && height <= 10) return null;
    const left = Math.min(dragSelect.startX, dragSelect.currentX);
    const top = Math.min(dragSelect.startY, dragSelect.currentY);
    return { left, top, width, height };
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

  // Context menu handlers
  const handleContextMenu = (e: React.MouseEvent, item: HistoryItem) => {
    e.preventDefault();
    if (item.isLoading) return;
    // If right-clicked item is in selection, use all selected; otherwise just this one
    if (selectedPaths.has(item.path)) {
      const items = history.filter((h) => selectedPaths.has(h.path));
      setContextMenu({ x: e.clientX, y: e.clientY, items });
    } else {
      setContextMenu({ x: e.clientX, y: e.clientY, items: [item] });
    }
  };

  const closeContextMenu = () => setContextMenu(null);

  const handleMenuReveal = () => {
    if (contextMenu && contextMenu.items.length > 0) {
      // Reveal first item
      invoke("reveal_in_folder", { path: contextMenu.items[0].path });
      closeContextMenu();
    }
  };

  const handleMenuEditCaption = () => {
    if (contextMenu && contextMenu.items.length === 1) {
      invoke("open_caption_editor", { path: contextMenu.items[0].path });
      closeContextMenu();
    }
  };

  const handleMenuToggleSelect = () => {
    if (!contextMenu || contextMenu.items.length !== 1) return;
    const path = contextMenu.items[0].path;
    setSelectedPaths((prev) => {
      const next = new Set(prev);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
    closeContextMenu();
  };

  const handleMenuDelete = async () => {
    if (!contextMenu) return;
    const { items } = contextMenu;
    const paths = items.map((i) => i.path);
    closeContextMenu();
    try {
      // Delete all selected
      await Promise.all(paths.map((p) => invoke("delete_file", { path: p })));
      // Remove from list
      const pathSet = new Set(paths);
      setHistory((prev) => prev.filter((h) => !pathSet.has(h.path)));
      setSelectedPaths(new Set());
      if (selected && pathSet.has(selected.path)) setSelected(null);
      loadStats();
      loadFolders();
    } catch (e) {
      console.error("删除失败:", e);
    }
  };

  // Folder operations
  const handleCreateFolder = async () => {
    if (!newFolderName.trim()) return;
    try {
      await invoke("create_folder", { name: newFolderName.trim() });
      setNewFolderName("");
      setIsCreatingFolder(false);
      loadFolders();
    } catch (e) {
      console.error("创建文件夹失败:", e);
    }
  };

  const handleMoveToFolder = async (folderPath: string | null) => {
    if (!contextMenu) return;
    const paths = contextMenu.items.map((i) => i.path);
    closeContextMenu();
    try {
      await invoke("move_to_folder", { filePaths: paths, folderPath });
      // Remove moved items from current view
      const pathSet = new Set(paths);
      setHistory((prev) => prev.filter((h) => !pathSet.has(h.path)));
      setSelectedPaths(new Set());
      if (selected && pathSet.has(selected.path)) setSelected(null);
      loadFolders();
    } catch (e) {
      console.error("移动失败:", e);
    }
  };

  // Folder menu handlers
  const handleFolderMenuClick = (e: React.MouseEvent, folder: FolderInfo) => {
    e.stopPropagation();
    setFolderMenu({ x: e.clientX, y: e.clientY, folder });
  };

  const closeFolderMenu = () => setFolderMenu(null);

  const handleStartRenameFolder = () => {
    if (!folderMenu) return;
    setEditingFolder({ path: folderMenu.folder.path, name: folderMenu.folder.name });
    closeFolderMenu();
  };

  const handleRenameFolder = async () => {
    if (!editingFolder || !editingFolder.name.trim()) return;
    try {
      await invoke("rename_folder", { path: editingFolder.path, newName: editingFolder.name.trim() });
      // If renaming selected folder, update selection
      if (selectedFolder === editingFolder.path) {
        const newPath = editingFolder.path.replace(/[^/]+$/, editingFolder.name.trim());
        setSelectedFolder(newPath);
      }
      loadFolders();
    } catch (e) {
      console.error("重命名失败:", e);
    } finally {
      setEditingFolder(null);
    }
  };

  const handleDeleteFolder = async () => {
    if (!folderMenu) return;
    const folderPath = folderMenu.folder.path;
    closeFolderMenu();
    try {
      await invoke("delete_folder", { path: folderPath });
      if (selectedFolder === folderPath) {
        setSelectedFolder(null);
      }
      loadFolders();
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      if (msg.includes("not empty")) {
        alert("文件夹不为空，无法删除");
      } else {
        console.error("删除失败:", e);
      }
    }
  };

  const handleExportFolderToMd = () => {
    if (!folderMenu) return;
    const { path, name } = folderMenu.folder;
    closeFolderMenu();
    setExportDialog({ folderPath: path, folderName: name });
  };

  const handleExportDialogClose = () => {
    setExportDialog(null);
  };

  const handleExported = async (path: string) => {
    setExportDialog(null);
    await invoke("reveal_in_folder", { path });
  };

  // Close context menu on click outside
  useEffect(() => {
    const handleClick = () => closeContextMenu();
    if (contextMenu) {
      window.addEventListener("click", handleClick);
      return () => window.removeEventListener("click", handleClick);
    }
  }, [contextMenu]);

  // Close folder menu on click outside
  useEffect(() => {
    const handleClick = () => closeFolderMenu();
    if (folderMenu) {
      window.addEventListener("click", handleClick);
      return () => window.removeEventListener("click", handleClick);
    }
  }, [folderMenu]);

  return (
    <main className={`dashboard ${isWideScreen ? "gallery-mode" : ""}`}>
      {/* Left Panel - List */}
      <div className="gallery-list" style={isWideScreen ? { width: sidebarWidth } : undefined}>
        {stats && (
          <>
            <div className="stats-compact" title="全部 | 截图 | GIF | 存储">
              {stats.total_count} | {stats.screenshot_count} | {stats.gif_count} | {formatSizeCompact(stats.total_size).value}{formatSizeCompact(stats.total_size).unit}
            </div>
            <div className="stats-grid">
              <div className="stat-card">
                <span className="stat-value">{stats.total_count}</span>
                <span className="stat-label">全部</span>
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
                <span className="stat-value">
                  {formatSizeCompact(stats.total_size).value}
                  <span className="stat-unit">{formatSizeCompact(stats.total_size).unit}</span>
                </span>
                <span className="stat-label">存储</span>
              </div>
            </div>
          </>
        )}

        {/* Folder List */}
        <div className="folder-section">
          <div className="folder-header">
            <span className="folder-title">文件夹</span>
            <button
              className="folder-add-btn"
              onClick={() => setIsCreatingFolder(true)}
              title="新建文件夹"
            >
              +
            </button>
          </div>
          {isCreatingFolder && (
            <div className="folder-create">
              <input
                type="text"
                value={newFolderName}
                onChange={(e) => setNewFolderName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") handleCreateFolder();
                  if (e.key === "Escape") {
                    setIsCreatingFolder(false);
                    setNewFolderName("");
                  }
                }}
                placeholder="文件夹名称"
                autoFocus
              />
              <button onClick={handleCreateFolder}>创建</button>
              <button onClick={() => { setIsCreatingFolder(false); setNewFolderName(""); }}>取消</button>
            </div>
          )}
          <div className="folder-list">
            <div
              className={`folder-item ${selectedFolder === null ? "active" : ""}`}
              onClick={() => setSelectedFolder(null)}
            >
              <span className="folder-icon">🏠</span>
              <span className="folder-name">全部文件</span>
            </div>
            {folders.map((folder) => (
              <div
                key={folder.path}
                className={`folder-item ${selectedFolder === folder.path ? "active" : ""}`}
                onClick={() => !editingFolder && setSelectedFolder(folder.path)}
              >
                <span className="folder-icon">📁</span>
                {editingFolder?.path === folder.path ? (
                  <input
                    type="text"
                    className="folder-rename-input"
                    value={editingFolder.name}
                    onChange={(e) => setEditingFolder({ ...editingFolder, name: e.target.value })}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") handleRenameFolder();
                      if (e.key === "Escape") setEditingFolder(null);
                    }}
                    onBlur={handleRenameFolder}
                    onClick={(e) => e.stopPropagation()}
                    autoFocus
                  />
                ) : (
                  <>
                    <span className="folder-name">{folder.name}</span>
                    <span className="folder-count">{folder.file_count}</span>
                    <button
                      className="folder-menu-btn"
                      onClick={(e) => handleFolderMenuClick(e, folder)}
                      title="更多操作"
                    >
                      ⋯
                    </button>
                  </>
                )}
              </div>
            ))}
          </div>
        </div>

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

          <div className="history-scroll" onMouseDown={handleDragStart}>
            {history.length === 0 && !loading ? (
              <div className="empty-state">
                <span className="empty-icon">📷</span>
                <p>暂无{filter === "gif" ? "GIF" : filter === "screenshot" ? "截图" : "记录"}</p>
                <p className="empty-hint">使用快捷键开始截图吧</p>
              </div>
            ) : (
              <Masonry
                breakpointCols={{ default: 3, 380: 4, 320: 3, 280: 2 }}
                className="history-grid"
                columnClassName="history-grid-column"
              >
                {history.map((item) => (
                  <div
                    key={item.path}
                    data-path={item.path}
                    className={`history-item ${selected?.path === item.path ? "selected" : ""} ${selectedPaths.has(item.path) ? "multi-selected" : ""} ${item.isLoading ? "loading" : ""} ${item.description ? "has-description" : ""}`}
                    onClick={(e) => !item.isLoading && handleItemClick(e, item)}
                    onContextMenu={(e) => handleContextMenu(e, item)}
                    title={isWideScreen ? item.filename : `${item.filename}\n${formatSize(item.size)}\n拖动框选`}
                  >
                    {item.isLoading ? (
                      <div className="history-thumb-loading">
                        <div className="loading-spinner" />
                      </div>
                    ) : (
                      <img src={convertFileSrc(item.path)} alt={item.filename} className="history-thumb" loading="lazy" />
                    )}
                    <span className={`history-badge history-badge-${item.file_type}`}>
                      {item.file_type === "gif" ? "GIF" : "IMG"}
                    </span>
                    {selectedPaths.size > 0 && (
                      <span className={`select-checkbox ${selectedPaths.has(item.path) ? "checked" : ""}`} />
                    )}
                    {item.description && (
                      <div className="history-description">{item.description}</div>
                    )}
                  </div>
                ))}
              </Masonry>
            )}

            {hasMore && (
              <div ref={loaderRef} className="history-loader">
                {loading ? "加载中..." : ""}
              </div>
            )}
          </div>
        </div>

        <div className="sidebar-footer">
          <span className="footer-brand">Lovshot</span>
          <span className="footer-hint"><kbd>⌥</kbd><kbd>A</kbd> 截图 · <kbd>⌥</kbd><kbd>G</kbd> GIF</span>
        </div>
      </div>

      {/* Resizer */}
      {isWideScreen && (
        <div className="gallery-resizer" onMouseDown={handleResizeStart} />
      )}

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
                  onLoad={(e) => {
                    const img = e.currentTarget;
                    setImageDimensions({ width: img.naturalWidth, height: img.naturalHeight });
                  }}
                />
              </div>
              <div className="preview-info">
                <h3 className="preview-filename">{selected.filename}</h3>
                <div className="preview-meta">
                  {imageDimensions && (
                    <>
                      <span className="meta-copyable" onClick={() => navigator.clipboard.writeText(`${imageDimensions.width}x${imageDimensions.height}`)} title="点击复制">
                        {imageDimensions.width} × {imageDimensions.height}
                      </span>
                      <span>·</span>
                    </>
                  )}
                  <span>{formatSize(selected.size)}</span>
                  <span>·</span>
                  <span>{formatDate(selected.modified)}</span>
                  <span>·</span>
                  <span className={`preview-type preview-type-${selected.file_type}`}>
                    {selected.file_type === "gif" ? "GIF" : "Screenshot"}
                  </span>
                </div>
                {selected.description && (
                  <p className="preview-description">{selected.description}</p>
                )}
                <div className="preview-actions">
                  <button className="btn-action" onClick={() => invoke("copy_image_to_clipboard", { path: selected.path })}>
                    复制
                  </button>
                  <button className="btn-action btn-secondary" onClick={handleOpenExternal}>
                    打开
                  </button>
                  <button className="btn-action btn-secondary" onClick={handleRevealInFinder}>
                    显示
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

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="context-menu"
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          {contextMenu.items.length > 1 && (
            <div className="context-menu-header">已选择 {contextMenu.items.length} 项</div>
          )}
          {contextMenu.items.length === 1 && (
            <button onClick={handleMenuToggleSelect}>
              {selectedPaths.has(contextMenu.items[0].path) ? "取消选择" : "选择"}
            </button>
          )}
          <button onClick={handleMenuReveal}>在 Finder 中显示</button>
          {contextMenu.items.length === 1 && (
            <button onClick={handleMenuEditCaption}>编辑描述</button>
          )}
          {/* Move to folder submenu */}
          <div className="context-menu-submenu">
            <button className="submenu-trigger">
              移动到文件夹 <span className="submenu-arrow">▶</span>
            </button>
            <div className="submenu-content">
              {selectedFolder !== null && (
                <button onClick={() => handleMoveToFolder(null)}>
                  🏠 根目录
                </button>
              )}
              {folders.filter(f => f.path !== selectedFolder).map((folder) => (
                <button key={folder.path} onClick={() => handleMoveToFolder(folder.path)}>
                  📁 {folder.name}
                </button>
              ))}
              {folders.length === 0 && selectedFolder === null && (
                <div className="submenu-empty">暂无文件夹</div>
              )}
            </div>
          </div>
          <div className="context-menu-divider" />
          <button className="danger" onClick={handleMenuDelete}>
            删除{contextMenu.items.length > 1 ? ` (${contextMenu.items.length})` : ""}
          </button>
        </div>
      )}

      {/* Folder Menu */}
      {folderMenu && (
        <div
          className="context-menu folder-menu"
          style={{ left: folderMenu.x, top: folderMenu.y }}
          onClick={(e) => e.stopPropagation()}
        >
          <button onClick={handleStartRenameFolder}>重命名</button>
          <button onClick={handleExportFolderToMd}>导出为 Markdown</button>
          <div className="context-menu-divider" />
          <button className="danger" onClick={handleDeleteFolder}>
            删除文件夹
          </button>
        </div>
      )}

      {/* Drag selection box */}
      {(() => {
        const style = getSelectionBoxStyle();
        return style && <div className="selection-box" style={style} />;
      })()}

      {/* Export Dialog */}
      {exportDialog && (
        <ExportDialog
          folderPath={exportDialog.folderPath}
          folderName={exportDialog.folderName}
          onClose={handleExportDialogClose}
          onExported={handleExported}
        />
      )}
    </main>
  );
}

export default App;
