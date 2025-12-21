import { useRef, useEffect, memo } from "react";
import { invoke } from "@tauri-apps/api/core";

interface MagnifierProps {
  /** 当前光标位置（逻辑像素） */
  cursorX: number;
  cursorY: number;
  /** 屏幕尺寸 */
  screenWidth: number;
  screenHeight: number;
  /** 是否正在拖拽选区 */
  isDragging?: boolean;
}

// 放大镜配置
const MAGNIFIER_SIZE = 120; // 放大镜显示尺寸
const ZOOM_LEVEL = 8; // 放大倍率
const SOURCE_SIZE = Math.floor(MAGNIFIER_SIZE / ZOOM_LEVEL); // 源区域尺寸 (15x15 逻辑像素)
const OFFSET = 20; // 光标偏移距离
const INFO_HEIGHT = 30; // 信息栏高度

/**
 * 放大镜组件 - 直接从 Rust 获取小区域像素，无需传输整个屏幕
 */
function MagnifierComponent({
  cursorX,
  cursorY,
  screenWidth,
  screenHeight,
  isDragging = false,
}: MagnifierProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const lastPosRef = useRef({ x: -1, y: -1 });

  // 获取并绘制像素
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    // 节流：位置变化小于 1px 时跳过
    const dx = Math.abs(cursorX - lastPosRef.current.x);
    const dy = Math.abs(cursorY - lastPosRef.current.y);
    if (dx < 1 && dy < 1) return;
    lastPosRef.current = { x: cursorX, y: cursorY };

    // 请求小区域像素（SOURCE_SIZE 逻辑像素）
    invoke<number[] | null>("get_magnifier_pixels", {
      x: Math.round(cursorX),
      y: Math.round(cursorY),
      size: SOURCE_SIZE,
    }).then((pixels) => {
      if (!pixels || pixels.length === 0) return;

      // 计算实际尺寸（像素数组是物理像素）
      const dpr = window.devicePixelRatio || 1;
      const physicalSize = Math.floor(SOURCE_SIZE * dpr);
      const actualSize = Math.sqrt(pixels.length / 4);

      // 创建 ImageData
      const imageData = new ImageData(
        new Uint8ClampedArray(pixels),
        actualSize,
        actualSize
      );

      // 清空并绘制
      ctx.clearRect(0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE);
      ctx.imageSmoothingEnabled = false;

      // 先绘制到临时 canvas，再放大
      const tempCanvas = document.createElement("canvas");
      tempCanvas.width = actualSize;
      tempCanvas.height = actualSize;
      const tempCtx = tempCanvas.getContext("2d");
      if (tempCtx) {
        tempCtx.putImageData(imageData, 0, 0);
        ctx.drawImage(tempCanvas, 0, 0, MAGNIFIER_SIZE, MAGNIFIER_SIZE);
      }

      // 绘制中心十字准星
      ctx.strokeStyle = "rgba(0, 0, 0, 0.8)";
      ctx.lineWidth = 1;
      const center = MAGNIFIER_SIZE / 2;
      const crossSize = ZOOM_LEVEL;

      ctx.beginPath();
      ctx.moveTo(center, center - crossSize * 2);
      ctx.lineTo(center, center + crossSize * 2);
      ctx.stroke();

      ctx.beginPath();
      ctx.moveTo(center - crossSize * 2, center);
      ctx.lineTo(center + crossSize * 2, center);
      ctx.stroke();

      // 中心像素高亮框
      ctx.strokeStyle = "rgba(204, 120, 92, 0.9)";
      ctx.lineWidth = 2;
      ctx.strokeRect(
        center - crossSize / 2,
        center - crossSize / 2,
        crossSize,
        crossSize
      );
    });
  }, [cursorX, cursorY]);

  // 计算放大镜位置
  const totalHeight = MAGNIFIER_SIZE + INFO_HEIGHT;
  let magX = cursorX - OFFSET - MAGNIFIER_SIZE;
  let magY = cursorY - OFFSET - totalHeight;

  if (magX < 8) magX = cursorX + OFFSET;
  if (magY < 8) magY = cursorY + OFFSET;

  magX = Math.min(magX, screenWidth - MAGNIFIER_SIZE - 8);
  magY = Math.min(magY, screenHeight - totalHeight - 8);

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
