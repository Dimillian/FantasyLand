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
  const shapes={bone:'<path d="M14 14Q5 5 5 16Q3 24 13 25L37 49Q35 60 45 58Q55 63 57 52Q64 43 52 40L27 17Q28 4 18 7Z" fill="#cec6a4" stroke="#756d57" stroke-width="3"/><path d="M20 23L42 45" stroke="#ede5c3" stroke-width="4"/>',fang:'<path d="M17 7L44 10Q48 34 18 58Q28 34 17 7" fill="#d8d0a6" stroke="#7a7253" stroke-width="3"/><path d="M25 16L33 19L25 43" fill="#f0e4b6"/>',ectoplasm:'<path d="M31 6Q9 30 12 46Q17 59 32 57Q56 59 51 42Q47 32 31 6" fill="#579d91" stroke="#9cd5ae" stroke-width="3"/><path d="M26 24L21 40L29 44L35 27" fill="#b6e0bf"/>',essence:'<path d="M32 3L49 25L43 49L31 61L13 38L17 17Z" fill="#8664b0" stroke="#bca1e4" stroke-width="2"/><path d="M32 8L25 31L33 52L40 29Z" fill="#dfcef5"/><path d="M4 13L8 18M52 5L49 13M55 45L61 47" stroke="#c9b9e9" stroke-width="2"/>'};
  if(shapes[name])return `<svg class="pixel-icon" viewBox="0 0 64 64" aria-hidden="true">${shapes[name]}</svg>`;
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
    aria-label="${escapeText(name)} XP"
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
    for (const [name, glyph, description] of group.skills) {
      const level=1,xp=0,next=75;
      const id = `${section.dataset.skillGroup}-${glyph}`;
      const skill = { name, glyph, description, level, xp, next, group: group.name, active: name==='Blades'||name==='Blocking' };
      skills.set(id, skill);
      const row = document.createElement('button');
      row.type = 'button';
      row.className = `skill-row${skill.active?'':' skill-inactive'}`;
      row.dataset.skill = id;
      row.setAttribute('aria-label', `${name}, level ${level}, ${xp} of ${next} XP`);
      row.innerHTML = `
        <span class="skill-icon">${icon(glyph)}</span>
        <span class="skill-info">
          <span class="skill-name">${escapeText(name)}</span>
          <span class="skill-level">${skill.active?`Rank ${level}`:'Untrained'}</span>
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

let currentCharacter=null,inventoryCategory='all',currentLoot=null;
function itemMarkup(item,location="inventory"){
  const stats=item.mainStats.map(s=>`<div><strong>${escapeText(s.value)}<em>${escapeText(s.suffix)}</em></strong><span>${escapeText(s.label)}</span></div>`).join('');
  const detailStats=[...item.details.filter(s=>s.label!=='Weight'),{label:'Unit weight',value:(item.weight/item.quantity).toFixed(2),suffix:'kg'},{label:'Stack weight',value:item.weight.toFixed(2),suffix:'kg'}].filter((v,i,a)=>a.findIndex(x=>x.label===v.label)===i);
  const details=detailStats.map(s=>`<div><dt>${escapeText(s.label)}</dt><dd>${escapeText(s.value)} <small>${escapeText(s.suffix)}</small></dd></div>`).join('');
  return `<div class="item-title-row"><span class="item-portrait">${icon(item.icon)}</span><div><span class="item-tier">${escapeText(item.tier)}</span><h3>${escapeText(item.name)}</h3><span class="item-category">${escapeText(item.category)} · ${escapeText(item.slot)}</span></div></div><div class="item-main-stats">${stats}</div><dl class="item-stat-list">${details}</dl><p class="item-flavor">${escapeText(item.description)}</p><div class="item-state"><span class="item-status-dot ${item.equipped?'on':''}"></span>${location==='body'?'On body':item.equipped?'Equipped':'In backpack'}<span>${escapeText(item.slot)}</span></div><div class="item-action">${item.group==='equipment'?(item.equipped?'Click or Enter to unequip':'Click or Enter to equip'):'Crafting material · No current use'}</div>`;
}
function renderTooltip(tooltip, trigger, skills) {
  const item=trigger.dataset.lootId?currentLoot?.items.find(item=>item.id===trigger.dataset.lootId):currentCharacter?.items.find(item=>item.id===trigger.dataset.item);
  tooltip.classList.toggle('item-tooltip',!!item);tooltip.dataset.tier=item?.tier||'';
  if(item){tooltip.innerHTML=itemMarkup(item,trigger.dataset.lootId?'body':'inventory');if(trigger.dataset.lootId)tooltip.querySelector('.item-action').textContent='Click or Enter to take';return;}
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
      <footer>${skill.active ? 'Grows through successful combat actions. Rank rewards come later.' : 'Not trainable yet'}</footer>`;
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
  if(!pack)return;
  // One tab stop for the pack; arrows navigate cells without 48 Tab presses.
  const cells = [...pack.querySelectorAll('[role="gridcell"]')];
  const compact=matchMedia('(max-width: 600px)');
  function layout(){
    const columns=compact.matches?6:12;
    pack.style.setProperty('--pack-columns',columns);
    pack.setAttribute('aria-colcount',columns);pack.setAttribute('aria-rowcount',Math.ceil(cells.length/columns));
    const focused=document.activeElement;
    pack.replaceChildren();
    for(let i=0;i<cells.length;i+=columns){
      const row=document.createElement('div');row.className='inventory-grid-row';row.setAttribute('role','row');
      cells.slice(i,i+columns).forEach((cell,j)=>{cell.setAttribute('aria-rowindex',i/columns+1);cell.setAttribute('aria-colindex',j+1);row.append(cell);});
      pack.append(row);
    }
    if(cells.includes(focused))focused.focus();
  }
  compact.addEventListener('change',layout);layout();
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
for (const trigger of document.querySelectorAll('[data-skill], [data-tip-title], [data-item]'))
  tips.bind(trigger);
bindInventory(document.querySelector('.inventory-grid'));

// Updates are emitted after authoritative Rust state changes, never invented UI XP.
document.addEventListener('character-progress',({detail:d})=>{
 currentCharacter=d;
 renderInventory();
 for(const [id,key,next] of [['combat-blade','blades',d.bladesNext],['combat-shield','blocking',d.blockingNext]]){
  const skill=skills.get(id);if(!skill)continue;const v=d.character[key];Object.assign(skill,{level:v.rank,xp:v.xp,next});
  const row=document.querySelector(`[data-skill="${id}"]`);row.setAttribute('aria-label',`${skill.name}, rank ${v.rank}, ${v.xp} of ${next} XP`);
  row.querySelector('.skill-level').textContent=`Rank ${v.rank}`;
  const bar=row.querySelector('.skill-xp');bar.setAttribute('aria-valuemax',next);bar.setAttribute('aria-valuenow',v.xp);bar.querySelector('i').style.width=`${v.xp/next*100}%`;
 }
});

function renderInventory(){
 if(!currentCharacter)return;
 const items=currentCharacter.items.filter(i=>inventoryCategory==='all'||i.group===inventoryCategory);
 for(const [index,slot] of [...document.querySelectorAll('.inventory-slot')].entries()){
  const item=items[index];slot.classList.toggle('occupied',!!item);slot.classList.toggle('is-equipped',!!item?.equipped);
  slot.removeAttribute('data-owned-item');slot.removeAttribute('aria-selected');
  if(item){slot.dataset.item=item.id;slot.dataset.tier=item.tier;slot.dataset.tipTitle=item.name;slot.setAttribute('aria-label',`${item.name} · ${item.quantity}${item.equipped?' · Equipped':''}`);slot.innerHTML=`<span class="icon-host">${icon(item.icon)}</span>${item.quantity>1?`<span class="slot-quantity">${item.quantity}</span>`:''}<span class="slot-equipped">${item.equipped?'EQUIPPED':''}</span>`;}
  else{delete slot.dataset.item;delete slot.dataset.tier;slot.dataset.tipTitle='Empty slot';slot.setAttribute('aria-label',`Empty slot ${index+1}`);slot.replaceChildren();}
 }
 document.getElementById('pack-category').textContent={all:'ALL ITEMS',equipment:'EQUIPMENT',parts:'MONSTER PARTS',essences:'ESSENCES'}[inventoryCategory];
 const equipped=currentCharacter.items.filter(i=>i.equipped).length;
 document.getElementById('pack-weight').textContent=`${currentCharacter.weight.toFixed(2)} kg carried`;
 document.getElementById('pack-equipped-count').textContent=`${equipped} equipped`;
 document.getElementById('pack-count').textContent=`${currentCharacter.items.length} / 48`;
 document.querySelector('.inventory-grid').setAttribute('aria-label',`Inventory, ${items.length} visible item stacks, ${equipped} equipped`);
 for(const b of document.querySelectorAll('[data-inventory-category]'))b.setAttribute('aria-pressed',String(b.dataset.inventoryCategory===inventoryCategory));
}
for(const b of document.querySelectorAll('[data-inventory-category]'))b.addEventListener('click',()=>{inventoryCategory=b.dataset.inventoryCategory;renderInventory();document.dispatchEvent(new Event('dismiss-item-tooltip'));});
document.addEventListener('loot-content',({detail})=>{
 currentLoot=detail;document.getElementById('loot-title').textContent=detail.name;
 document.getElementById('loot-weight').textContent=`${detail.items.reduce((a,i)=>a+i.weight,0).toFixed(2)} kg remaining`;
 const grid=document.getElementById('loot-items');grid.replaceChildren();
 for(const item of detail.items){const b=document.createElement('button');b.type='button';b.className='loot-item';b.dataset.lootId=item.id;b.dataset.tier=item.tier;b.setAttribute('aria-label',`Take ${item.quantity} ${item.name}`);b.innerHTML=`<span class="loot-icon">${icon(item.icon)}</span><span><small>${escapeText(item.category)}</small><strong>${escapeText(item.name)}</strong><em>${item.quantity} × ${ (item.weight/item.quantity).toFixed(2)} kg</em></span><b>+ ${item.quantity}</b>`;b.addEventListener('click',()=>document.dispatchEvent(new CustomEvent('loot-take',{detail:item.id})));tips.bind(b);grid.append(b);}
 if(!detail.items.length)grid.innerHTML='<p class="loot-empty">Nothing remains.</p>';
 document.getElementById('loot-all').disabled=!detail.items.length;
 document.dispatchEvent(new Event('dismiss-item-tooltip'));
});
