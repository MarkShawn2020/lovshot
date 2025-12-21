import { useRef, useEffect, useState, memo } from "react";

interface MagnifierProps {
  /** 全屏截图的 base64 data URL */
  screenshot: string;
  /** 当前光标位置（逻辑像素） */
  cursorX: number;
  cursorY: number;
  /** 屏幕尺寸 */
  screenWidth: number;
  screenHeight: number;
  /** 设备像素比 */
  devicePixelRatio?: number;
  /** 是否正在拖拽选区 */
  isDragging?: boolean;
}

// 放大镜配置
const MAGNIFIER_SIZE = 120; // 放大镜显示尺寸
const ZOOM_LEVEL = 8; // 放大倍率
const SOURCE_SIZE = Math.floor(MAGNIFIER_SIZE / ZOOM_LEVEL); // 源区域尺寸 (15x15)
const OFFSET = 20; // 光标偏移距离
const INFO_HEIGHT = 30; // 信息栏高度

/**
 * 放大镜组件 - 显示光标位置附近的像素放大效果
 * 用于精确选择截图区域的起点和终点
 */
function MagnifierComponent({
  screenshot,
  cursorX,
  cursorY,
  screenWidth,
  screenHeight,
  devicePixelRatio = window.devicePixelRatio || 1,
  isDragging = false,
}: MagnifierProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const imgRef = useRef<HTMLImageElement | null>(null);
  const [imageLoaded, setImageLoaded] = useState(false);

  // 加载截图
  useEffect(() => {
    if (!screenshot) return;

    const img = new Image();
    img.onload = () => {
      imgRef.current = img;
      setImageLoaded(true);
    };
    img.src = screenshot;

    return () => {
      imgRef.current = null;
      setImageLoaded(false);
    };
  }, [screenshot]);

  // 绘制放大镜内容
  useEffect(() => {
    const canvas = canvasRef.current;
    const img = imgRef.current;
    if (!canvas || !img || !imageLoaded) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // 清空画布
    ctx.clearRect(0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE);

    // 计算源图片中的采样区域（考虑设备像素比）
    // 截图是物理像素尺寸，光标位置是逻辑像素
    const srcX = Math.floor(cursorX * devicePixelRatio - SOURCE_SIZE / 2);
    const srcY = Math.floor(cursorY * devicePixelRatio - SOURCE_SIZE / 2);

    // 禁用图像平滑以获得像素化效果
    ctx.imageSmoothingEnabled = false;

    // 从源图片采样并放大绘制
    ctx.drawImage(
      img,
      srcX, srcY, SOURCE_SIZE, SOURCE_SIZE,  // 源区域
      0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE   // 目标区域
    );

    // 绘制中心十字准星
    ctx.strokeStyle = "rgba(0, 0, 0, 0.8)";
    ctx.lineWidth = 1;
    const center = MAGNIFIER_SIZE / 2;
    const crossSize = ZOOM_LEVEL; // 一个像素的大小

    // 垂直线
    ctx.beginPath();
    ctx.moveTo(center, center - crossSize * 2);
    ctx.lineTo(center, center + crossSize * 2);
    ctx.stroke();

    // 水平线
    ctx.beginPath();
    ctx.moveTo(center - crossSize * 2, center);
    ctx.lineTo(center + crossSize * 2, center);
    ctx.stroke();

    // 中心像素高亮框
    ctx.strokeStyle = "rgba(204, 120, 92, 0.9)"; // primary color
    ctx.lineWidth = 2;
    ctx.strokeRect(
      center - crossSize / 2,
      center - crossSize / 2,
      crossSize,
      crossSize
    );
  }, [cursorX, cursorY, devicePixelRatio, imageLoaded]);

  // 计算放大镜位置（默认左上角，空间不够时自适应）
  const totalHeight = MAGNIFIER_SIZE + INFO_HEIGHT;

  // 默认：左上角（光标左上方）
  let magX = cursorX - OFFSET - MAGNIFIER_SIZE;
  let magY = cursorY - OFFSET - totalHeight;

  // 如果左侧空间不够，改为右侧
  if (magX < 8) {
    magX = cursorX + OFFSET;
  }

  // 如果上方空间不够，改为下方
  if (magY < 8) {
    magY = cursorY + OFFSET;
  }

  // 最终边界检查
  magX = Math.min(magX, screenWidth - MAGNIFIER_SIZE - 8);
  magY = Math.min(magY, screenHeight - totalHeight - 8);

  if (!screenshot || !imageLoaded) return null;

  return (
    <div
      className="magnifier"
      style={{
        left: magX,
        top: magY,
      }}
    >
      <canvas
        ref={canvasRef}
        width={MAGNIFIER_SIZE}
        height={MAGNIFIER_SIZE}
        className="magnifier-canvas"
      />
      <div className="magnifier-info">
        <span className="magnifier-coords">
          {Math.round(cursorX)}, {Math.round(cursorY)}
        </span>
        {isDragging && <span className="magnifier-hint">终点</span>}
      </div>
    </div>
  );
}

export const Magnifier = memo(MagnifierComponent);
