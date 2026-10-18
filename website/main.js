// theme
document.querySelector('.theme-btn')?.addEventListener('click', () => {
  const r = document.documentElement, n = r.dataset.theme === 'light' ? 'dark' : 'light';
  r.dataset.theme = n;
  try { localStorage.setItem('os-theme', n); } catch (e) {}
});

// interactive treemap demo
(() => {
  const cv = document.getElementById('map');
  if (!cv) return;
  const CATS = [['Images', '#e8648f'], ['Video', '#8b6cf0'], ['Audio', '#2fbfa8'], ['Archives', '#e8a33d'], ['Code', '#4a90e2'], ['Documents', '#5fbf6a'], ['Apps', '#e5694f']];
  const f = (n, s, c) => ({ n, s, c });
  const many = (p, c, k, a, b) => Array.from({ length: k }, (_, i) => f(`${p}${i + 1}`, a + ((i * 37) % (b - a)), c));
  const d = (n, ch) => ({ n, ch });
  const root = d('alex', [
    d('Movies', [f('Wedding-2024.mov', 190, 1), f('Trip-to-Iceland.mp4', 120, 1), f('Drone-raw.mkv', 80, 1), f('Screen-recording.mov', 40, 1)]),
    d('Pictures', [d('Photos Library', [...many('IMG_', 0, 60, 2, 7), ...many('DSC_', 0, 16, 9, 25)])]),
    d('Music', many('Track-', 2, 26, 6, 19)),
    d('Downloads', [f('ubuntu-24.04.iso', 150, 3), f('backup-2023.zip', 60, 3), f('installer.dmg', 45, 3), f('data-export.csv', 14, 5), f('slides.key', 22, 5), f('report-final.pdf', 8, 5)]),
    d('Developer', [d('web-app', [d('node_modules', many('pkg-', 4, 110, 1, 3)), d('src', many('module', 4, 25, 1, 2))]), d('rust-game', [d('target', [f('debug.rlib', 95, 4), ...many('dep', 4, 20, 3, 13)])])]),
    d('Library', [d('Caches', many('cache-', 6, 40, 2, 15))]),
    d('Applications', [d('Studio.app', [f('studio', 85, 6), f('core.dylib', 55, 6)])]),
    d('Documents', [d('Work', [...many('proposal-', 5, 15, 1, 6), f('budget.xlsx', 6, 5)]), d('Taxes', many('return-', 5, 10, 1, 5))]),
  ]);
  const size = (n) => n.s ?? (n.s = n.ch.reduce((a, c) => a + size(c), 0));
  const parent = new Map();
  (function walk(n) { n.ch?.forEach((c) => { parent.set(c, n); walk(c); }); })(root);
  const fmt = (m) => (m >= 1000 ? (m / 1000).toFixed(2) + ' GB' : Math.round(m) + ' MB');
  size(root);
  let view = root, cells = [], hl = -1, hover = null;
  const tip = document.getElementById('tip'), ctx = cv.getContext('2d');
  const css = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();

  function squarify(kids, x, y, w, h, depth) {
    const tot = kids.reduce((a, k) => a + k.s, 0), sc = (w * h) / tot;
    let i = 0;
    while (i < kids.length && w > 1 && h > 1) {
      const side = Math.min(w, h);
      let sum = 0, j = i, worst = Infinity, mn = Infinity, mx = 0;
      while (j < kids.length) {
        const a = kids[j].s * sc, ns = sum + a, nmn = Math.min(mn, a), nmx = Math.max(mx, a);
        const wr = Math.max((side * side * nmx) / (ns * ns), (ns * ns) / (side * side * nmn));
        if (j > i && wr > worst) break;
        sum = ns; mn = nmn; mx = nmx; worst = wr; j++;
      }
      const horiz = w >= h, th = sum / side;
      let off = 0;
      for (let k = i; k < j; k++) {
        const len = ((kids[k].s * sc) / sum) * side;
        const r = horiz ? [x, y + off, th, len] : [x + off, y, len, th];
        off += len;
        place(kids[k], ...r, depth);
      }
      if (horiz) { x += th; w -= th; } else { y += th; h -= th; }
      i = j;
    }
  }
  function place(n, x, y, w, h, depth) {
    if (w < 1 || h < 1) return;
    if (n.ch && w > 14 && h > 14 && depth < 12) {
      const hd = w > 46 && h > 38 ? 15 : 1.5;
      cells.push({ n, x, y, w, h, frame: true, hd, depth });
      if (w - 3 > 4 && h - 1.5 - hd > 4) squarify([...n.ch].sort((a, b) => b.s - a.s), x + 1.5, y + hd, w - 3, h - hd - 1.5, depth + 1);
    } else cells.push({ n, x, y, w, h, depth });
  }
  function leafCat(n) { return n.c; }

  function draw() {
    const dpr = devicePixelRatio || 1, W = cv.clientWidth, H = cv.clientHeight;
    cv.width = W * dpr; cv.height = H * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    cells = [];
    place(view, 0, 0, W, H, 0);
    ctx.fillStyle = '#090c12'; ctx.fillRect(0, 0, W, H);
    ctx.font = '11.5px ui-sans-serif, system-ui, sans-serif'; ctx.textBaseline = 'top';
    for (const c of cells) {
      if (c.frame) {
        ctx.fillStyle = c.depth % 2 ? '#19222f' : '#141b28'; ctx.fillRect(c.x, c.y, c.w, c.h);
        ctx.strokeStyle = '#26314a'; ctx.strokeRect(c.x + .5, c.y + .5, c.w - 1, c.h - 1);
        if (c.hd > 10) { ctx.fillStyle = '#94a0b4'; ctx.save(); ctx.beginPath(); ctx.rect(c.x, c.y, c.w - 3, 15); ctx.clip(); ctx.fillText(c.n.n, c.x + 4, c.y + 2); ctx.restore(); }
        continue;
      }
      let base = c.n.ch ? '#303e58' : CATS[leafCat(c.n)][1];
      const g = ctx.createLinearGradient(c.x, c.y, c.x + c.w, c.y + c.h);
      g.addColorStop(0, '#ffffff30'); g.addColorStop(1, '#00000040');
      ctx.fillStyle = base; ctx.fillRect(c.x + .6, c.y + .6, c.w - 1.2, c.h - 1.2);
      ctx.fillStyle = g; ctx.fillRect(c.x + .6, c.y + .6, c.w - 1.2, c.h - 1.2);
      if (hl >= 0 && (c.n.ch || leafCat(c.n) !== hl)) { ctx.fillStyle = '#090c12c4'; ctx.fillRect(c.x, c.y, c.w, c.h); }
      if (c.w > 54 && c.h > 18 && !c.n.ch && (hl < 0 || leafCat(c.n) === hl)) {
        ctx.save(); ctx.beginPath(); ctx.rect(c.x, c.y, c.w - 3, c.h); ctx.clip();
        ctx.fillStyle = '#080c14e6'; ctx.fillText(c.n.n, c.x + 4, c.y + 3);
        if (c.h > 34) { ctx.fillStyle = '#080c1499'; ctx.fillText(fmt(c.n.s), c.x + 4, c.y + 17); }
        ctx.restore();
      }
    }
    if (hover) { ctx.strokeStyle = '#3b82c4'; ctx.lineWidth = 2; ctx.strokeRect(hover.x + 1, hover.y + 1, hover.w - 2, hover.h - 2); ctx.lineWidth = 1; }
    const crumbs = []; for (let n = view; n; n = parent.get(n)) crumbs.unshift(n);
    const cr = document.getElementById('crumb'); cr.textContent = '';
    crumbs.forEach((n, i) => {
      if (i) cr.append(' › ');
      const b = document.createElement('button'); b.textContent = n.n; b.onclick = () => { view = n; draw(); }; cr.append(b);
    });
    document.getElementById('dsize').textContent = fmt(view.s);
  }
  const at = (e) => { const r = cv.getBoundingClientRect(), x = e.clientX - r.left, y = e.clientY - r.top; for (let i = cells.length - 1; i >= 0; i--) { const c = cells[i]; if (x >= c.x && x < c.x + c.w && y >= c.y && y < c.y + c.h) return c; } return null; };
  cv.addEventListener('mousemove', (e) => {
    const c = at(e); if (c !== hover) { hover = c; draw(); }
    if (!c) { tip.style.display = 'none'; return; }
    tip.innerHTML = '';
    const b = document.createElement('b'); b.textContent = c.n.n;
    const s = document.createElement('small'); s.textContent = `${fmt(c.n.s)} · ${(c.n.s * 100 / root.s).toFixed(1)}% of scan` + (c.n.ch ? '' : ' · ' + CATS[c.n.c][0]);
    tip.append(b, s);
    tip.style.display = 'block'; tip.style.left = Math.min(e.clientX + 14, innerWidth - 300) + 'px'; tip.style.top = e.clientY + 14 + 'px';
  });
  cv.addEventListener('mouseleave', () => { hover = null; tip.style.display = 'none'; draw(); });
  cv.addEventListener('click', (e) => { const c = at(e); if (c?.n.ch && c.n !== view) { view = c.n; hover = null; draw(); } else if (!c?.n.ch && view !== root && c) { /* leaf click: stay */ } });
  cv.addEventListener('dblclick', () => { const p = parent.get(view); if (p) { view = p; draw(); } });

  const lg = document.getElementById('legend');
  const tot = new Array(7).fill(0);
  (function t(n) { n.ch ? n.ch.forEach(t) : (tot[n.c] += n.s); })(root);
  CATS.map((c, i) => [c, i]).sort((a, b) => tot[b[1]] - tot[a[1]]).forEach(([c, i]) => {
    const s = document.createElement('span'); s.style.cursor = 'pointer';
    const sw = document.createElement('i'); sw.style.background = c[1];
    s.append(sw, `${c[0]}  ${fmt(tot[i])}`);
    s.onclick = () => { hl = hl === i ? -1 : i; s.parentNode.querySelectorAll('span').forEach((x) => (x.style.opacity = hl < 0 || x === s ? 1 : .45)); draw(); };
    lg.append(s);
  });
  addEventListener('resize', draw);
  draw();
})();

document.querySelectorAll('.copy').forEach((b) => b.addEventListener('click', async () => {
  try { await navigator.clipboard.writeText(b.dataset.copy); b.textContent = 'Copied'; setTimeout(() => (b.textContent = 'Copy'), 1600); } catch (e) {}
}));

// live GitHub star count
fetch('https://api.github.com/repos/MarshallNickolauson/OpenStorage').then((r) => r.json()).then((d) => {
  if (typeof d.stargazers_count !== 'number') return;
  const n = new Intl.NumberFormat('en', { notation: 'compact' }).format(d.stargazers_count);
  document.querySelectorAll('[data-stars]').forEach((el) => { el.querySelector('span').textContent = n; el.hidden = false; });
}).catch(() => {});
