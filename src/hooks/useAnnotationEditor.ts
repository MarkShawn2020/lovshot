import { useState, useCallback, useRef } from 'react';
import type {
  Annotation,
  AnnotationTool,
  AnnotationStyles,
  RectStyle,
  ArrowStyle,
  MosaicStyle,
} from '../types/annotation';
import { DEFAULT_STYLES, ANNOTATION_COLORS } from '../types/annotation';

const MAX_HISTORY = 50;

export function useAnnotationEditor() {
  const [annotations, setAnnotations] = useState<Annotation[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeTool, setActiveTool] = useState<AnnotationTool>('select');
  const [activeColor, setActiveColor] = useState(ANNOTATION_COLORS[0].value);
  const [activeStyles, setActiveStyles] = useState<AnnotationStyles>(DEFAULT_STYLES);
  const [strokeWidth, setStrokeWidth] = useState(2);
  const [fontSize, setFontSize] = useState(16);

  // History for undo/redo
  const historyRef = useRef<Annotation[][]>([[]]);
  const historyIndexRef = useRef(0);

  // Use ref to track selectedId for stable callbacks
  const selectedIdRef = useRef<string | null>(null);
  selectedIdRef.current = selectedId;

  // Stable pushHistory using functional update
  const pushHistory = useCallback((updater: (prev: Annotation[]) => Annotation[]) => {
    setAnnotations(prev => {
      const newAnnotations = updater(prev);

      const history = historyRef.current;
      const index = historyIndexRef.current;

      // Truncate future history
      const newHistory = history.slice(0, index + 1);
      newHistory.push(newAnnotations);

      // Limit history size
      if (newHistory.length > MAX_HISTORY) {
        newHistory.shift();
      } else {
        historyIndexRef.current = newHistory.length - 1;
      }

      historyRef.current = newHistory;
      return newAnnotations;
    });
  }, []);

  // Stable callbacks - no dependencies on annotations
  const addAnnotation = useCallback((annotation: Annotation) => {
    pushHistory(prev => [...prev, annotation]);
  }, [pushHistory]);

  const updateAnnotation = useCallback((id: string, updates: Partial<Annotation>) => {
    pushHistory(prev => prev.map(ann =>
      ann.id === id ? { ...ann, ...updates } as Annotation : ann
    ));
  }, [pushHistory]);

  const deleteAnnotation = useCallback((id: string) => {
    pushHistory(prev => prev.filter(ann => ann.id !== id));
    if (selectedIdRef.current === id) {
      setSelectedId(null);
    }
  }, [pushHistory]);

  const deleteSelected = useCallback(() => {
    if (selectedIdRef.current) {
      deleteAnnotation(selectedIdRef.current);
    }
  }, [deleteAnnotation]);

  const undo = useCallback(() => {
    const index = historyIndexRef.current;
    if (index > 0) {
      historyIndexRef.current = index - 1;
      setAnnotations(historyRef.current[index - 1]);
      setSelectedId(null);
    }
  }, []);

  const redo = useCallback(() => {
    const history = historyRef.current;
    const index = historyIndexRef.current;
    if (index < history.length - 1) {
      historyIndexRef.current = index + 1;
      setAnnotations(history[index + 1]);
      setSelectedId(null);
    }
  }, []);

  const setRectStyle = useCallback((style: RectStyle) => {
    setActiveStyles((prev) => ({ ...prev, rect: style }));
  }, []);

  const setArrowStyle = useCallback((style: ArrowStyle) => {
    setActiveStyles((prev) => ({ ...prev, arrow: style }));
  }, []);

  const setMosaicStyle = useCallback((style: MosaicStyle) => {
    setActiveStyles((prev) => ({ ...prev, mosaic: style }));
  }, []);

  const reset = useCallback(() => {
    setAnnotations([]);
    setSelectedId(null);
    setActiveTool('select');
    historyRef.current = [[]];
    historyIndexRef.current = 0;
  }, []);

  const canUndo = historyIndexRef.current > 0;
  const canRedo = historyIndexRef.current < historyRef.current.length - 1;

  return {
    // State
    annotations,
    selectedId,
    activeTool,
    activeColor,
    activeStyles,
    strokeWidth,
    fontSize,
    canUndo,
    canRedo,

    // Setters
    setSelectedId,
    setActiveTool,
    setActiveColor,
    setStrokeWidth,
    setFontSize,
    setRectStyle,
    setArrowStyle,
    setMosaicStyle,

    // Actions
    addAnnotation,
    updateAnnotation,
    deleteAnnotation,
    deleteSelected,
    undo,
    redo,
    reset,
  };
}

export type AnnotationEditor = ReturnType<typeof useAnnotationEditor>;
