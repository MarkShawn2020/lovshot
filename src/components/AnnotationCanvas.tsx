import { useRef, useState, useEffect, useCallback } from 'react';
import { Stage, Layer, Image, Rect, Arrow, Text, Line, Transformer, Shape, Group } from 'react-konva';
import type Konva from 'konva';
import type {
  Annotation,
  AnnotationTool,
  AnnotationStyles,
  RectAnnotation,
  ArrowAnnotation,
  TextAnnotation,
  MosaicAnnotation,
} from '../types/annotation';

interface Props {
  imageUrl: string;
  width: number;
  height: number;
  left: number;
  top: number;
  annotations: Annotation[];
  selectedId: string | null;
  activeTool: AnnotationTool;
  activeColor: string;
  activeStyles: AnnotationStyles;
  strokeWidth: number;
  fontSize: number;
  onAddAnnotation: (annotation: Annotation) => void;
  onUpdateAnnotation: (id: string, updates: Partial<Annotation>) => void;
  onDeleteAnnotation: (id: string) => void;
  onSelectAnnotation: (id: string | null) => void;
  stageRef: React.RefObject<Konva.Stage | null>;
}

interface DrawingState {
  isDrawing: boolean;
  startX: number;
  startY: number;
  currentAnnotation: Annotation | null;
}

function generateId() {
  return `ann_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
}

export function AnnotationCanvas({
  imageUrl,
  width,
  height,
  left,
  top,
  annotations,
  selectedId,
  activeTool,
  activeColor,
  activeStyles,
  strokeWidth,
  fontSize,
  onAddAnnotation,
  onUpdateAnnotation,
  onDeleteAnnotation,
  onSelectAnnotation,
  stageRef,
}: Props) {
  const [image, setImage] = useState<HTMLImageElement | null>(null);
  const [drawing, setDrawing] = useState<DrawingState>({
    isDrawing: false,
    startX: 0,
    startY: 0,
    currentAnnotation: null,
  });
  const [editingTextId, setEditingTextId] = useState<string | null>(null);
  const [hoveredTextId, setHoveredTextId] = useState<string | null>(null);

  const transformerRef = useRef<Konva.Transformer>(null);
  const textInputRef = useRef<HTMLTextAreaElement>(null);
  const isComposingRef = useRef(false);
  const isDraggingRef = useRef(false);

  // Focus textarea when editing text
  useEffect(() => {
    if (editingTextId && textInputRef.current) {
      // Use setTimeout to ensure textarea is rendered and ready
      setTimeout(() => {
        textInputRef.current?.focus();
      }, 50);
    }
  }, [editingTextId]);

  // Load background image
  useEffect(() => {
    const img = new window.Image();
    img.src = imageUrl;
    img.onload = () => setImage(img);
  }, [imageUrl]);

  // Update transformer when selection changes
  useEffect(() => {
    const transformer = transformerRef.current;
    const stage = stageRef.current;
    if (!transformer || !stage) return;

    if (selectedId && activeTool === 'select') {
      const node = stage.findOne(`#${selectedId}`);
      if (node) {
        transformer.nodes([node]);
        transformer.getLayer()?.batchDraw();
        return;
      }
    }
    transformer.nodes([]);
    transformer.getLayer()?.batchDraw();
  }, [selectedId, activeTool, annotations, stageRef]);

  const handleMouseDown = useCallback((e: Konva.KonvaEventObject<MouseEvent>) => {
    // Check if clicked on empty space (Stage background)
    const clickedOnEmpty = e.target === e.target.getStage() || e.target.name() === 'background';

    // If editing text, finish current edit first (always)
    if (editingTextId) {
      const currentText = textInputRef.current?.value || '';
      if (currentText.trim()) {
        onUpdateAnnotation(editingTextId, { text: currentText });
      } else {
        // Delete empty annotation
        onDeleteAnnotation(editingTextId);
      }
      setEditingTextId(null);

      // If not clicking on empty space or not using text tool, stop here
      if (!clickedOnEmpty || activeTool !== 'text') {
        return;
      }
      // Otherwise continue to create new text at clicked position
    }

    if (activeTool === 'select') {
      if (clickedOnEmpty) {
        onSelectAnnotation(null);
      }
      return;
    }

    // Only create new annotation when clicking on empty space
    if (!clickedOnEmpty) {
      return;
    }

    const stage = e.target.getStage();
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;

    const id = generateId();

    if (activeTool === 'rect') {
      const annotation: RectAnnotation = {
        id,
        type: 'rect',
        x: pos.x,
        y: pos.y,
        width: 0,
        height: 0,
        style: activeStyles.rect,
        color: activeColor,
        strokeWidth,
      };
      setDrawing({ isDrawing: true, startX: pos.x, startY: pos.y, currentAnnotation: annotation });
    } else if (activeTool === 'mosaic') {
      const annotation: MosaicAnnotation = {
        id,
        type: 'mosaic',
        x: pos.x,
        y: pos.y,
        width: 0,
        height: 0,
        style: activeStyles.mosaic,
        blockSize: 10,
      };
      setDrawing({ isDrawing: true, startX: pos.x, startY: pos.y, currentAnnotation: annotation });
    } else if (activeTool === 'arrow') {
      const annotation: ArrowAnnotation = {
        id,
        type: 'arrow',
        points: [pos.x, pos.y, pos.x, pos.y],
        style: activeStyles.arrow,
        color: activeColor,
        strokeWidth,
      };
      setDrawing({ isDrawing: true, startX: pos.x, startY: pos.y, currentAnnotation: annotation });
    } else if (activeTool === 'text') {
      const annotation: TextAnnotation = {
        id,
        type: 'text',
        x: pos.x,
        y: pos.y,
        text: '',
        fontSize,
        color: activeColor,
      };
      onAddAnnotation(annotation);
      setEditingTextId(id);
      onSelectAnnotation(id);
    }
  }, [activeTool, activeColor, activeStyles, strokeWidth, fontSize, onAddAnnotation, onSelectAnnotation, onUpdateAnnotation, onDeleteAnnotation, editingTextId]);

  const handleMouseMove = useCallback((e: Konva.KonvaEventObject<MouseEvent>) => {
    if (!drawing.isDrawing || !drawing.currentAnnotation) return;

    const stage = e.target.getStage();
    if (!stage) return;
    const pos = stage.getPointerPosition();
    if (!pos) return;

    const ann = drawing.currentAnnotation;

    if (ann.type === 'rect' || ann.type === 'mosaic') {
      const x = Math.min(pos.x, drawing.startX);
      const y = Math.min(pos.y, drawing.startY);
      const w = Math.abs(pos.x - drawing.startX);
      const h = Math.abs(pos.y - drawing.startY);
      setDrawing({
        ...drawing,
        currentAnnotation: { ...ann, x, y, width: w, height: h } as typeof ann,
      });
    } else if (ann.type === 'arrow') {
      setDrawing({
        ...drawing,
        currentAnnotation: {
          ...ann,
          points: [drawing.startX, drawing.startY, pos.x, pos.y],
        } as ArrowAnnotation,
      });
    }
  }, [drawing]);

  const handleMouseUp = useCallback(() => {
    if (!drawing.isDrawing || !drawing.currentAnnotation) return;

    const ann = drawing.currentAnnotation;

    // Only add if has meaningful size
    if (ann.type === 'rect' || ann.type === 'mosaic') {
      if ((ann as RectAnnotation).width > 5 && (ann as RectAnnotation).height > 5) {
        onAddAnnotation(ann);
      }
    } else if (ann.type === 'arrow') {
      const pts = (ann as ArrowAnnotation).points;
      const dx = pts[2] - pts[0];
      const dy = pts[3] - pts[1];
      if (Math.sqrt(dx * dx + dy * dy) > 10) {
        onAddAnnotation(ann);
      }
    }

    setDrawing({ isDrawing: false, startX: 0, startY: 0, currentAnnotation: null });
  }, [drawing, onAddAnnotation]);

  const handleShapeClick = useCallback((id: string) => {
    if (activeTool === 'select') {
      onSelectAnnotation(id);
    }
  }, [activeTool, onSelectAnnotation]);

  const handleTransformEnd = useCallback((id: string, node: Konva.Node) => {
    const scaleX = node.scaleX();
    const scaleY = node.scaleY();

    // Reset scale and apply to dimensions
    node.scaleX(1);
    node.scaleY(1);

    const ann = annotations.find((a) => a.id === id);
    if (!ann) return;

    if (ann.type === 'rect' || ann.type === 'mosaic') {
      onUpdateAnnotation(id, {
        x: node.x(),
        y: node.y(),
        width: Math.max(5, node.width() * scaleX),
        height: Math.max(5, node.height() * scaleY),
      });
    }
  }, [annotations, onUpdateAnnotation]);

  // Handle text edit completion
  // fromBlur: when true, don't delete empty annotations (let handleMouseDown/ESC do it)
  const handleTextEdit = useCallback((id: string, text: string, fromBlur = false) => {
    if (text.trim()) {
      onUpdateAnnotation(id, { text });
      setEditingTextId(null);
    } else if (!fromBlur) {
      // Only delete empty annotations when explicitly requested (Enter key)
      onDeleteAnnotation(id);
      setEditingTextId(null);
    }
    // If fromBlur and empty text: do nothing, keep textarea open
  }, [onUpdateAnnotation, onDeleteAnnotation]);

  const renderAnnotation = (ann: Annotation) => {
    if (ann.type === 'rect') {
      const dashArray = ann.style === 'dashed' ? [10, 5] : undefined;
      const fill = ann.style === 'filled' ? `${ann.color}40` : undefined;
      return (
        <Rect
          key={ann.id}
          id={ann.id}
          x={ann.x}
          y={ann.y}
          width={ann.width}
          height={ann.height}
          stroke={ann.color}
          strokeWidth={ann.strokeWidth}
          dash={dashArray}
          fill={fill}
          draggable={activeTool === 'select'}
          onClick={() => handleShapeClick(ann.id)}
          onTap={() => handleShapeClick(ann.id)}
          onDragEnd={(e) => {
            onUpdateAnnotation(ann.id, { x: e.target.x(), y: e.target.y() });
          }}
          onTransformEnd={(e) => handleTransformEnd(ann.id, e.target)}
        />
      );
    }

    if (ann.type === 'mosaic') {
      return (
        <MosaicShape
          key={ann.id}
          annotation={ann}
          backgroundImage={image}
          activeTool={activeTool}
          onClick={() => handleShapeClick(ann.id)}
          onDragEnd={(x, y) => onUpdateAnnotation(ann.id, { x, y })}
          onTransformEnd={(w, h) => onUpdateAnnotation(ann.id, { width: w, height: h })}
        />
      );
    }

    if (ann.type === 'arrow') {
      const pointerLength = ann.style === 'thick' ? 15 : 10;
      const pointerWidth = ann.style === 'thick' ? 12 : 8;
      const sw = ann.style === 'thick' ? ann.strokeWidth * 2 : ann.strokeWidth;

      if (ann.style === 'double') {
        // Double-headed arrow using Line + custom pointers
        return (
          <Line
            key={ann.id}
            id={ann.id}
            points={ann.points}
            stroke={ann.color}
            strokeWidth={sw}
            lineCap="round"
            lineJoin="round"
            draggable={activeTool === 'select'}
            onClick={() => handleShapeClick(ann.id)}
            onTap={() => handleShapeClick(ann.id)}
            onDragEnd={(e) => {
              const dx = e.target.x();
              const dy = e.target.y();
              e.target.position({ x: 0, y: 0 });
              onUpdateAnnotation(ann.id, {
                points: [
                  ann.points[0] + dx,
                  ann.points[1] + dy,
                  ann.points[2] + dx,
                  ann.points[3] + dy,
                ],
              });
            }}
          />
        );
      }

      return (
        <Arrow
          key={ann.id}
          id={ann.id}
          points={ann.points}
          stroke={ann.color}
          fill={ann.color}
          strokeWidth={sw}
          pointerLength={pointerLength}
          pointerWidth={pointerWidth}
          lineCap="round"
          lineJoin="round"
          draggable={activeTool === 'select'}
          onClick={() => handleShapeClick(ann.id)}
          onTap={() => handleShapeClick(ann.id)}
          onDragEnd={(e) => {
            const dx = e.target.x();
            const dy = e.target.y();
            e.target.position({ x: 0, y: 0 });
            onUpdateAnnotation(ann.id, {
              points: [
                ann.points[0] + dx,
                ann.points[1] + dy,
                ann.points[2] + dx,
                ann.points[3] + dy,
              ],
            });
          }}
        />
      );
    }

    if (ann.type === 'text') {
      const isEditing = editingTextId === ann.id;
      const hasText = !!ann.text;
      const isHovered = hoveredTextId === ann.id;
      const isDraggable = hasText && !isEditing;

      // Calculate text dimensions for border
      const padding = 4;
      const textNode = stageRef.current?.findOne(`#${ann.id}`) as Konva.Text | undefined;
      const textWidth = textNode?.width() || 100;
      const textHeight = textNode?.height() || ann.fontSize;

      return (
        <Group
          key={ann.id}
          x={ann.x}
          y={ann.y}
          draggable={isDraggable}
          onDragStart={() => { isDraggingRef.current = true; }}
          onDragEnd={(e) => {
            onUpdateAnnotation(ann.id, { x: e.target.x(), y: e.target.y() });
            setTimeout(() => { isDraggingRef.current = false; }, 100);
          }}
          onMouseEnter={(e) => {
            if (isDraggable) {
              setHoveredTextId(ann.id);
              const container = e.target.getStage()?.container();
              if (container) container.style.cursor = 'move';
            }
          }}
          onMouseLeave={(e) => {
            setHoveredTextId(null);
            const container = e.target.getStage()?.container();
            if (container) container.style.cursor = 'default';
          }}
        >
          {/* Hover border */}
          {isHovered && isDraggable && (
            <Rect
              x={-padding}
              y={-padding}
              width={textWidth + padding * 2}
              height={textHeight + padding * 2}
              stroke="#0066ff"
              strokeWidth={1}
              dash={[4, 4]}
              fill="transparent"
            />
          )}
          <Text
            id={ann.id}
            x={0}
            y={0}
            text={ann.text || (isEditing ? '' : '点击输入')}
            fontSize={ann.fontSize}
            fill={ann.color}
            onClick={() => {
              handleShapeClick(ann.id);
              if (activeTool === 'select' && hasText) {
                setEditingTextId(ann.id);
              }
            }}
            onTap={() => handleShapeClick(ann.id)}
            onDblClick={() => setEditingTextId(ann.id)}
            onDblTap={() => setEditingTextId(ann.id)}
            opacity={hasText ? 1 : 0.5}
          />
        </Group>
      );
    }

    return null;
  };

  // Render current drawing annotation
  const renderDrawing = () => {
    if (!drawing.currentAnnotation) return null;
    return renderAnnotation(drawing.currentAnnotation);
  };

  // Text editing overlay
  const editingText = editingTextId ? annotations.find((a) => a.id === editingTextId && a.type === 'text') as TextAnnotation | undefined : null;

  return (
    <div
      className="annotation-canvas"
      style={{
        position: 'fixed',
        left,
        top,
        width,
        height,
        zIndex: 15,
      }}
    >
      <Stage
        ref={stageRef}
        width={width}
        height={height}
        onMouseDown={handleMouseDown}
        onMouseMove={handleMouseMove}
        onMouseUp={handleMouseUp}
        onMouseLeave={handleMouseUp}
      >
        <Layer>
          {/* Background image */}
          {image && (
            <Image
              image={image}
              width={width}
              height={height}
              name="background"
            />
          )}
        </Layer>
        <Layer>
          {/* Annotations */}
          {annotations.map(renderAnnotation)}
          {renderDrawing()}

          {/* Transformer for selection */}
          <Transformer
            ref={transformerRef}
            boundBoxFunc={(oldBox, newBox) => {
              // Limit resize
              if (newBox.width < 5 || newBox.height < 5) {
                return oldBox;
              }
              return newBox;
            }}
          />
        </Layer>
      </Stage>

      {/* Text editing textarea overlay */}
      {editingText && (
        <textarea
          ref={textInputRef}
          className="annotation-text-input"
          style={{
            position: 'absolute',
            left: editingText.x,
            top: editingText.y,
            fontSize: editingText.fontSize,
            color: editingText.color,
            minWidth: 100,
            minHeight: editingText.fontSize + 8,
          }}
          defaultValue={editingText.text}
          autoFocus
          onCompositionStart={() => { isComposingRef.current = true; }}
          onCompositionEnd={() => { isComposingRef.current = false; }}
          onBlur={(e) => handleTextEdit(editingText.id, e.target.value, true)}
          onKeyDown={(e) => {
            // Ignore during IME composition (e.g., Chinese pinyin input)
            // keyCode 229 = IME processing, isComposing = composition in progress
            if (
              e.nativeEvent.isComposing ||
              e.keyCode === 229 ||
              isComposingRef.current
            ) {
              return;
            }

            if (e.key === 'Escape') {
              // Cancel input, delete annotation
              e.preventDefault();
              e.stopPropagation();
              onDeleteAnnotation(editingText.id);
              setEditingTextId(null);
            } else if (e.key === 'Enter' && !e.shiftKey) {
              // Enter to confirm, Shift+Enter for newline
              e.preventDefault();
              handleTextEdit(editingText.id, e.currentTarget.value);
            }
          }}
        />
      )}
    </div>
  );
}

// Mosaic custom shape
interface MosaicShapeProps {
  annotation: MosaicAnnotation;
  backgroundImage: HTMLImageElement | null;
  activeTool: AnnotationTool;
  onClick: () => void;
  onDragEnd: (x: number, y: number) => void;
  onTransformEnd: (width: number, height: number) => void;
}

function MosaicShape({ annotation, backgroundImage, activeTool, onClick, onDragEnd, onTransformEnd: _onTransformEnd }: MosaicShapeProps) {
  const { id, x, y, width, height, style, blockSize } = annotation;

  if (!backgroundImage || width < 1 || height < 1) return null;

  return (
    <Shape
      id={id}
      x={x}
      y={y}
      width={width}
      height={height}
      draggable={activeTool === 'select'}
      onClick={onClick}
      onTap={onClick}
      onDragEnd={(e) => onDragEnd(e.target.x(), e.target.y())}
      sceneFunc={(ctx, shape) => {
        const w = shape.width();
        const h = shape.height();

        if (style === 'blur') {
          // Simple blur effect using multiple semi-transparent draws
          ctx.save();
          ctx.filter = 'blur(8px)';
          ctx.drawImage(
            backgroundImage,
            x, y, w, h,
            0, 0, w, h
          );
          ctx.restore();
        } else {
          // Pixelate effect
          const bs = blockSize;
          const cols = Math.ceil(w / bs);
          const rows = Math.ceil(h / bs);

          // Create offscreen canvas to sample pixels
          const offscreen = document.createElement('canvas');
          offscreen.width = backgroundImage.width;
          offscreen.height = backgroundImage.height;
          const offCtx = offscreen.getContext('2d');
          if (!offCtx) return;
          offCtx.drawImage(backgroundImage, 0, 0);

          const imgData = offCtx.getImageData(
            Math.floor(x),
            Math.floor(y),
            Math.ceil(w),
            Math.ceil(h)
          );

          for (let row = 0; row < rows; row++) {
            for (let col = 0; col < cols; col++) {
              const px = col * bs;
              const py = row * bs;
              const bw = Math.min(bs, w - px);
              const bh = Math.min(bs, h - py);

              // Sample center pixel
              const sx = Math.floor(px + bw / 2);
              const sy = Math.floor(py + bh / 2);
              const idx = (sy * Math.ceil(w) + sx) * 4;

              if (idx >= 0 && idx < imgData.data.length - 3) {
                const r = imgData.data[idx];
                const g = imgData.data[idx + 1];
                const b = imgData.data[idx + 2];
                ctx.fillStyle = `rgb(${r},${g},${b})`;
                ctx.fillRect(px, py, bw, bh);
              }
            }
          }
        }

        ctx.fillStrokeShape(shape);
      }}
    />
  );
}

export default AnnotationCanvas;
