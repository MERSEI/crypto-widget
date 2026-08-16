import { useCallback, useRef } from "react";
import { commands } from "../../core/ipc/commands";

const DRAG_THRESHOLD_PX = 4;

/**
 * A single pointerdown region needs to serve two gestures: drag-to-reposition and
 * click-to-toggle. Calling `start_dragging()` unconditionally on every pointerdown (the
 * previous approach) hands the window to the OS move-loop immediately and swallows the
 * subsequent click — so a plain tap never reaches `onClick`. Instead we wait for real
 * pointer movement past a small threshold before starting the OS drag; if the pointer is
 * released before that, it's treated as a click.
 */
export function usePillDrag(onClick?: () => void) {
  const startPos = useRef<{ x: number; y: number } | null>(null);
  const draggingRef = useRef(false);

  const onPointerDown = useCallback(
    (event: React.PointerEvent) => {
      if (event.button !== 0) return;
      // Let buttons (pin/settings/collapse/…) handle their own clicks untouched.
      if ((event.target as HTMLElement).closest("button")) return;

      startPos.current = { x: event.clientX, y: event.clientY };
      draggingRef.current = false;

      const onPointerMove = (moveEvent: PointerEvent) => {
        if (!startPos.current || draggingRef.current) return;
        const dx = moveEvent.clientX - startPos.current.x;
        const dy = moveEvent.clientY - startPos.current.y;
        if (Math.hypot(dx, dy) > DRAG_THRESHOLD_PX) {
          draggingRef.current = true;
          void commands.startDrag();
        }
      };

      const onPointerUp = () => {
        window.removeEventListener("pointermove", onPointerMove);
        window.removeEventListener("pointerup", onPointerUp);
        if (draggingRef.current) {
          // Give the OS drag a beat to settle the final window position before we read it.
          setTimeout(() => void commands.dragEnded(), 50);
        } else {
          onClick?.();
        }
        startPos.current = null;
        draggingRef.current = false;
      };

      window.addEventListener("pointermove", onPointerMove);
      window.addEventListener("pointerup", onPointerUp);
    },
    [onClick],
  );

  return { onPointerDown };
}
