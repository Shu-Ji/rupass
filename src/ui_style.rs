pub(crate) const STYLE_CSS: &str = r##":root {
  --bg-top: #eef8ff;
  --bg-bottom: #d7ebf5;
  --panel: rgba(255, 255, 255, 0.88);
  --panel-strong: #ffffff;
  --text: #083344;
  --muted: #4a6b78;
  --line: rgba(12, 74, 110, 0.12);
  --line-strong: rgba(3, 105, 161, 0.26);
  --primary: #0369a1;
  --secondary: #0ea5e9;
  --cta: #22c55e;
  --cta-dark: #15803d;
  --danger: #be123c;
  --danger-soft: #ffe4e6;
  --primary-soft: rgba(3, 105, 161, 0.09);
  --shadow-lg: 0 24px 60px rgba(8, 51, 68, 0.12);
  --shadow-sm: 0 10px 20px rgba(8, 51, 68, 0.08);
}

* { box-sizing: border-box; }

html { color-scheme: light; }

body {
  margin: 0;
  min-height: 100vh;
  color: var(--text);
  background:
    radial-gradient(circle at top left, rgba(14, 165, 233, 0.18) 0, transparent 28%),
    linear-gradient(180deg, var(--bg-top) 0%, var(--bg-bottom) 100%);
  font-family: "Fira Sans", "Avenir Next", "PingFang SC", "Noto Sans SC", sans-serif;
}

button, input, textarea {
  font: inherit;
}

button {
  appearance: none;
  border: 0;
  border-radius: 14px;
  padding: 10px 14px;
  cursor: pointer;
  background: linear-gradient(180deg, var(--cta) 0%, var(--cta-dark) 100%);
  color: #fff;
  box-shadow: var(--shadow-sm);
  transition: transform 180ms ease, box-shadow 180ms ease, opacity 180ms ease;
}

button:hover {
  opacity: 0.95;
  transform: translateY(-1px);
}

button:active {
  transform: translateY(1px);
}

button.secondary {
  color: var(--text);
  background: linear-gradient(180deg, #ffffff 0%, #dff5ff 100%);
}

button.danger {
  background: linear-gradient(180deg, #e11d48 0%, #be123c 100%);
}

button.ghost {
  color: var(--text);
  background: rgba(255, 255, 255, 0.55);
  border: 1px solid var(--line-strong);
  box-shadow: none;
}

button:focus-visible,
input:focus-visible,
textarea:focus-visible,
.skip-link:focus-visible,
.team-button:focus-visible,
.secret-button:focus-visible {
  outline: 3px solid rgba(14, 165, 233, 0.32);
  outline-offset: 2px;
}

input, textarea {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 11px 12px;
  background: var(--panel-strong);
  color: var(--text);
  transition: border-color 180ms ease, box-shadow 180ms ease;
}

input:focus,
textarea:focus {
  border-color: var(--secondary);
  box-shadow: 0 0 0 4px rgba(14, 165, 233, 0.12);
}

textarea {
  min-height: 200px;
  resize: vertical;
  font-family: "Fira Code", "SFMono-Regular", monospace;
}

.skip-link {
  position: absolute;
  left: 16px;
  top: -44px;
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--text);
  color: #fff;
  text-decoration: none;
}

.skip-link:focus {
  top: 16px;
}

.shell {
  max-width: 1440px;
  margin: 0 auto;
  padding: 28px;
}

.hero {
  display: grid;
  grid-template-columns: minmax(280px, 1.2fr) minmax(280px, 0.9fr) minmax(220px, 0.7fr);
  gap: 18px;
  margin-bottom: 18px;
}

.hero-copy,
.hero-stats,
.hero-status,
.card {
  background: var(--panel);
  border: 1px solid rgba(255, 255, 255, 0.54);
  border-radius: 28px;
  box-shadow: var(--shadow-lg);
  backdrop-filter: blur(12px);
}

.hero-copy,
.hero-status {
  padding: 22px;
}

.hero-stats {
  padding: 14px;
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 12px;
}

.eyebrow,
.stat-label {
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: var(--secondary);
}

.hero h1 {
  margin: 8px 0 10px;
  font-size: 34px;
  line-height: 1.05;
}

.hero p,
.muted,
.status-note {
  color: var(--muted);
}

.hero p {
  margin: 0;
  line-height: 1.6;
  max-width: 52ch;
}

.stat-card {
  padding: 16px;
  border-radius: 20px;
  background: linear-gradient(180deg, rgba(255,255,255,0.86) 0%, rgba(224, 247, 255, 0.86) 100%);
  border: 1px solid rgba(14, 165, 233, 0.18);
}

.stat-card strong {
  display: block;
  margin-top: 8px;
  font: 700 20px/1.1 "Fira Code", "SFMono-Regular", monospace;
}

.status {
  min-height: 24px;
  margin-top: 12px;
  font-weight: 600;
}

.status.error {
  color: var(--danger);
}

.layout {
  display: grid;
  grid-template-columns: 340px minmax(0, 1fr);
  gap: 18px;
}

.card-body {
  padding: 18px;
}

.stack,
.workspace {
  display: grid;
  gap: 14px;
}

.section-head {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  gap: 12px;
}

.section-head h2,
.section-head h3 {
  margin: 0;
  font-size: 17px;
}

.toolbar {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  align-items: center;
}

.form-block {
  padding-top: 8px;
  border-top: 1px solid var(--line);
}

.team-list,
.secret-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 10px;
}

.team-button,
.secret-button {
  width: 100%;
  border: 1px solid var(--line);
  border-radius: 18px;
  padding: 14px;
  background: linear-gradient(180deg, rgba(255,255,255,0.84) 0%, rgba(243, 250, 255, 0.84) 100%);
  color: var(--text);
  text-align: left;
  box-shadow: none;
}

.team-button:hover,
.secret-button:hover {
  border-color: var(--line-strong);
  transform: translateY(-1px);
}

.team-button.active,
.secret-button.active {
  border-color: var(--primary);
  background: linear-gradient(180deg, rgba(224, 242, 254, 0.9) 0%, rgba(240, 249, 255, 0.96) 100%);
}

.team-name,
.secret-name {
  display: block;
  font: 700 14px/1.35 "Fira Code", "SFMono-Regular", monospace;
}

.secret-name {
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.team-subtitle {
  margin-top: 6px;
  font-size: 13px;
  color: var(--muted);
}

.grid-two {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.label {
  display: block;
  margin-bottom: 6px;
  font-size: 12px;
  font-weight: 700;
  letter-spacing: 0.06em;
  text-transform: uppercase;
  color: var(--muted);
}

.banner-body {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: center;
}

.banner-pills {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.pill {
  padding: 8px 10px;
  border-radius: 999px;
  background: var(--primary-soft);
  color: var(--primary);
  font: 700 12px/1 "Fira Code", "SFMono-Regular", monospace;
}

.secret-pane {
  display: grid;
  grid-template-columns: 320px minmax(0, 1fr);
  gap: 18px;
}

.action-cluster {
  align-self: end;
  justify-content: flex-end;
}

.empty {
  padding: 18px;
  border: 1px dashed var(--line-strong);
  border-radius: 18px;
  color: var(--muted);
  background: rgba(255, 255, 255, 0.45);
}

@media (prefers-reduced-motion: reduce) {
  * {
    transition: none !important;
    animation: none !important;
    scroll-behavior: auto !important;
  }
}

@media (max-width: 1120px) {
  .hero {
    grid-template-columns: 1fr;
  }

  .hero-stats {
    grid-template-columns: repeat(3, minmax(0, 1fr));
  }

  .layout,
  .secret-pane,
  .grid-two {
    grid-template-columns: 1fr;
  }

  .banner-body {
    align-items: flex-start;
    flex-direction: column;
  }
}

@media (max-width: 640px) {
  .shell {
    padding: 16px;
  }

  .hero-stats {
    grid-template-columns: 1fr;
  }

  .toolbar,
  .action-cluster {
    justify-content: stretch;
  }

  button {
    width: 100%;
  }
}
"##;
