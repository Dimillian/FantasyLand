// One anchored tooltip for pointer, keyboard and touch. It lives outside the
// scrolling frame, but inside the active dialog for aria-modal compatibility.
export function tooltipPosition(anchor, size, viewport) {
  const gap = 10,
    edge = 8;
  let x = anchor.right + gap,
    y = anchor.top;
  if (x + size.width > viewport.width - edge) x = anchor.left - size.width - gap;
  if (x < edge) {
    x = anchor.left;
    y = anchor.bottom + gap;
    if (y + size.height > viewport.height - edge) y = anchor.top - size.height - gap;
  }
  return {
    x: Math.max(edge, Math.min(x, viewport.width - size.width - edge)),
    y: Math.max(edge, Math.min(y, viewport.height - size.height - edge)),
  };
}

export function createTooltips(render) {
  const tooltip = document.createElement('div');
  tooltip.id = 'game-tooltip';
  tooltip.className = 'game-tooltip hidden';
  tooltip.setAttribute('role', 'tooltip');
  document.body.append(tooltip);
  let active = null,
    timer = null,
    previousDescription = null,
    suppressHover = false;
  function hide() {
    clearTimeout(timer);
    if (active) {
      if (previousDescription === null) active.removeAttribute('aria-describedby');
      else active.setAttribute('aria-describedby', previousDescription);
    }
    active = null;
    tooltip.classList.add('hidden');
  }
  function show(trigger) {
    clearTimeout(timer);
    if (active !== trigger) {
      hide();
      active = trigger;
      previousDescription = trigger.getAttribute('aria-describedby');
    }
    render(tooltip, trigger);
    (trigger.closest('.overlay') || document.body).append(tooltip);
    tooltip.classList.remove('hidden');
    trigger.setAttribute(
      'aria-describedby',
      [previousDescription, tooltip.id].filter(Boolean).join(' '),
    );
    const point = tooltipPosition(
      trigger.getBoundingClientRect(),
      tooltip.getBoundingClientRect(),
      { width: innerWidth, height: innerHeight },
    );
    tooltip.style.left = `${point.x}px`;
    tooltip.style.top = `${point.y}px`;
  }
  function deferHide(source) {
    // A late blur/leave from the previous control must not dismiss the new one.
    if (source !== active) return;
    clearTimeout(timer);
    timer = setTimeout(hide, 130);
  }
  function bind(trigger) {
    trigger.addEventListener('pointerenter', (event) => {
      if (event.pointerType !== 'touch' && !suppressHover) show(trigger);
    });
    trigger.addEventListener('pointerleave', () => {
      if (document.activeElement !== trigger) deferHide(trigger);
    });
    trigger.addEventListener('focus', () => {
      suppressHover = false;
      show(trigger);
    });
    trigger.addEventListener('blur', () => deferHide(trigger));
    // A tap focuses non-button inventory cells as well as opening their tooltip.
    trigger.addEventListener('click', () => {
      if (trigger.matches('[role="gridcell"]')) trigger.focus();
      show(trigger);
    });
  }
  tooltip.addEventListener('pointerenter', () => clearTimeout(timer));
  tooltip.addEventListener('pointerleave', () => {
    if (document.activeElement !== active) deferHide(active);
  });
  document.addEventListener(
    'keydown',
    (event) => {
      if (event.code === 'Escape' && active) {
        suppressHover = true;
        hide();
        event.preventDefault();
        event.stopImmediatePropagation();
      }
    },
    true,
  );
  // Dismissing a popup can reveal a different row under the stationary pointer.
  // Ignore that synthetic re-entry until the user actually moves the pointer.
  document.addEventListener('pointermove', (event) => {
    if (suppressHover && (event.movementX || event.movementY)) {
      suppressHover = false;
      const trigger = event.target.closest('[data-skill], [data-tip-title]');
      if (trigger && event.pointerType !== 'touch') show(trigger);
    }
  });
  document.addEventListener(
    'pointerdown',
    (event) => {
      if (active && !active.contains(event.target) && !tooltip.contains(event.target)) hide();
    },
    true,
  );
  document.addEventListener(
    'scroll',
    (event) => {
      if (!tooltip.contains(event.target)) hide();
    },
    true,
  );
  window.addEventListener('resize', hide);
  // Menus may close from hotkeys, pointer lock changes, or navigation buttons.
  const observer = new MutationObserver(() => {
    if (
      active &&
      (!active.getClientRects().length || getComputedStyle(active).visibility === 'hidden')
    )
      hide();
  });
  observer.observe(document.body, { attributes: true, attributeFilter: ['class'] });
  for (const panel of document.querySelectorAll('.overlay, [id^="options-"]')) {
    observer.observe(panel, { attributes: true, attributeFilter: ['class'] });
  }
  return { bind, hide };
}
