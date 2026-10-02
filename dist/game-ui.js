import { skillGroups } from './skill-catalog.js';
import { createTooltips } from './ui-tooltips.js';

function escapeText(text) {
  const entities = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
  return String(text).replace(/[&<>"']/g, (character) => entities[character]);
}

const equipmentIcons = new Set([
  'helmet',
  'armor',
  'gloves',
  'boots',
  'blade',
  'shield',
  'ring',
  'mining',
]);

function icon(name) {
  if (equipmentIcons.has(name)) {
    return `<svg class="pixel-icon equipment-icon" viewBox="0 0 64 64" aria-hidden="true">
      <use href="./ui/equipment-icons.svg#${escapeText(name)}"/>
    </svg>`;
  }
  return `<svg class="pixel-icon" viewBox="0 0 24 24" aria-hidden="true">
    <use href="./ui/icons.svg#${escapeText(name)}"/>
  </svg>`;
}

function experienceBar({ name, xp, next }) {
  return `<span class="skill-xp" role="progressbar"
    aria-label="${escapeText(name)} XP (preview)"
    aria-valuemin="0" aria-valuemax="${next}" aria-valuenow="${xp}">
    <i style="width:${(xp / next) * 100}%"></i>
  </span>`;
}

function renderSkills(container) {
  const skills = new Map();
  for (const group of skillGroups) {
    const section = document.createElement('section');
    section.className = 'skill-group';
    section.dataset.skillGroup = group.name.toLowerCase();
    section.innerHTML = `<h3>${escapeText(group.name)}</h3>`;
    for (const [name, glyph, description, level, xp, next] of group.skills) {
      const id = `${section.dataset.skillGroup}-${glyph}`;
      const skill = { name, glyph, description, level, xp, next, group: group.name };
      skills.set(id, skill);
      const row = document.createElement('button');
      row.type = 'button';
      row.className = 'skill-row';
      row.dataset.skill = id;
      row.setAttribute('aria-label', `${name}, level ${level}, ${xp} of ${next} XP, preview`);
      row.innerHTML = `
        <span class="skill-icon">${icon(glyph)}</span>
        <span class="skill-info">
          <span class="skill-name">${escapeText(name)}</span>
          <span class="skill-level">Lv. ${level}</span>
          ${experienceBar(skill)}
        </span>`;
      section.append(row);
    }
    container.append(section);
  }
  return skills;
}

function prepareSettingHelp() {
  // Keep live values beside their labels; expose explanatory copy on demand.
  for (const [index, row] of [...document.querySelectorAll('.setting-row')].entries()) {
    const control = row.querySelector('input, select');
    const help = row.querySelector('small');
    const label = row.querySelector('span');
    if (!control || !help || !label) continue;
    if (!help.id) help.id = `setting-help-${index}`;
    const title = label.cloneNode(true);
    for (const detail of title.querySelectorAll('small, output')) detail.remove();
    control.dataset.tipTitle =
      title.textContent.replace(/\s+/g, ' ').trim() ||
      control.getAttribute('aria-label') ||
      'Setting';
    control.dataset.tipSource = help.id;
    for (const output of help.querySelectorAll('output')) {
      output.classList.add('setting-value');
      label.append(output);
    }
    help.classList.add('tooltip-source');
  }
}

function renderTooltip(tooltip, trigger, skills) {
  const skill = skills.get(trigger.dataset.skill);
  tooltip.dataset.skillGroup = skill?.group.toLowerCase() || '';
  if (skill) {
    tooltip.innerHTML = `
      <div class="tooltip-heading">
        ${icon(skill.glyph)}
        <div><strong>${escapeText(skill.name)}</strong>
          <span>${escapeText(skill.group)} · Level ${skill.level}</span></div>
      </div>
      <p>${escapeText(skill.description)}</p>
      <div class="tooltip-xp-label"><span>Experience</span><b>${skill.xp} / ${skill.next}</b></div>
      ${experienceBar(skill)}
      <footer>Preview · progression not yet active</footer>`;
    return;
  }
  const title = trigger.dataset.tipTitle || 'Empty slot';
  const text = trigger.dataset.tipSource
    ? document.getElementById(trigger.dataset.tipSource)?.textContent.replace(/\s+/g, ' ').trim()
    : trigger.dataset.tipText;
  tooltip.innerHTML = `
    <div class="tooltip-heading">
      ${trigger.dataset.tipIcon ? icon(trigger.dataset.tipIcon) : ''}
      <strong>${escapeText(title)}</strong>
    </div>
    ${text ? `<p>${escapeText(text)}</p>` : ''}`;
}

function bindInventory(pack) {
  // One tab stop for the pack; arrows navigate cells without 48 Tab presses.
  const cells = [...pack.querySelectorAll('[role="gridcell"]')];
  function selectCell(index, focus = true) {
    for (const [i, cell] of cells.entries()) cell.tabIndex = i === index ? 0 : -1;
    if (focus) cells[index].focus();
  }
  pack.addEventListener('keydown', (event) => {
    const current = cells.indexOf(document.activeElement);
    if (current < 0) return;
    const columns = Number(getComputedStyle(pack).getPropertyValue('--pack-columns')) || 8;
    const delta = { ArrowRight: 1, ArrowLeft: -1, ArrowDown: columns, ArrowUp: -columns }[
      event.code
    ];
    let next =
      delta === undefined ? null : Math.min(cells.length - 1, Math.max(0, current + delta));
    if (event.code === 'Home') next = 0;
    if (event.code === 'End') next = cells.length - 1;
    if (next !== null) {
      event.preventDefault();
      event.stopPropagation();
      selectCell(next);
    }
  });
  for (const [index, cell] of cells.entries()) {
    cell.addEventListener('pointerdown', () => selectCell(index, false));
  }
}

const skills = renderSkills(document.getElementById('skill-groups'));
for (const host of document.querySelectorAll('[data-icon]')) {
  host.innerHTML = icon(host.dataset.icon);
}
prepareSettingHelp();
const tips = createTooltips((tooltip, trigger) => renderTooltip(tooltip, trigger, skills));
for (const trigger of document.querySelectorAll('[title]')) {
  trigger.dataset.tipTitle ||= trigger.getAttribute('title');
  trigger.removeAttribute('title');
}
for (const trigger of document.querySelectorAll('[data-skill], [data-tip-title]'))
  tips.bind(trigger);
bindInventory(document.querySelector('.inventory-grid'));
