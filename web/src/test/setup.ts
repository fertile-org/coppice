// jsdom lacks layout APIs that Radix (Popover/Dialog) and cmdk rely on.
if (typeof window !== 'undefined') {
  if (typeof globalThis.ResizeObserver === 'undefined') {
    class ResizeObserverStub {
      observe() {}
      unobserve() {}
      disconnect() {}
    }
    globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;
  }

  const elementProto = window.Element.prototype;
  if (!elementProto.scrollIntoView) {
    elementProto.scrollIntoView = function scrollIntoView() {};
  }
  if (!elementProto.hasPointerCapture) {
    elementProto.hasPointerCapture = function hasPointerCapture() {
      return false;
    };
  }
  if (!elementProto.setPointerCapture) {
    elementProto.setPointerCapture = function setPointerCapture() {};
  }
  if (!elementProto.releasePointerCapture) {
    elementProto.releasePointerCapture = function releasePointerCapture() {};
  }
}
