pub(crate) const SCRIPT_JS: &str = r##"const state = {
  teams: [],
  selectedTeam: null,
  secrets: [],
  passwords: {},
  selectedKey: "",
};

const $ = (id) => document.getElementById(id);

const teamButtonClass =
  "w-full rounded-[22px] border px-4 py-3 text-left shadow-sm transition focus:outline-none focus:ring-4 focus:ring-black/5";
const teamButtonIdle =
  "border-black/8 bg-white/88 text-ink hover:-translate-y-0.5 hover:border-black/12 hover:bg-white";
const teamButtonActive =
  "border-accent/30 bg-accentSoft text-ink ring-2 ring-black/5";

const secretButtonClass =
  "w-full rounded-[18px] border px-4 py-3 text-left shadow-sm transition focus:outline-none focus:ring-4 focus:ring-black/5";
const secretButtonIdle =
  "border-black/8 bg-white/88 text-ink hover:-translate-y-0.5 hover:border-black/12 hover:bg-white";
const secretButtonActive =
  "border-accent/30 bg-accentSoft text-ink ring-2 ring-black/5";

function renderIcons() {
  window.lucide?.createIcons();
}

function setStatus(message, isError = false) {
  const node = $("status");
  node.textContent = message || "";
  node.className = isError
    ? "min-h-11 rounded-[20px] border border-red-200/70 bg-red-50 px-4 py-3 text-sm font-medium text-red-700"
    : "min-h-11 rounded-[20px] border border-black/6 bg-white/75 px-4 py-3 text-sm font-medium text-mute";
}

async function request(path, options = {}) {
  const response = await fetch(path, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  const text = await response.text();
  let data = {};
  if (text) {
    try {
      data = JSON.parse(text);
    } catch {
      data = { message: text };
    }
  }
  if (!response.ok) {
    throw new Error(data.error || data.message || "request failed");
  }
  return data;
}

function currentTeam() {
  return state.teams.find((team) => team.team_name === state.selectedTeam) || null;
}

function requireTeam() {
  const team = currentTeam();
  if (!team) throw new Error("请先选择团队");
  return team;
}

function requirePassword(teamName) {
  const password = state.passwords[teamName] || $("teamPassword").value.trim();
  if (!password) throw new Error("请输入团队密码");
  state.passwords[teamName] = password;
  return password;
}

function renderOverview() {
  $("teamCount").textContent = String(state.teams.length);
  $("keyCount").textContent = String(state.secrets.length);
  $("activeTeamBadge").textContent = state.selectedTeam || "none";
}

function renderTeams() {
  const list = $("teamList");
  list.innerHTML = "";
  if (!state.teams.length) {
    list.innerHTML =
      '<li class="rounded-[22px] border border-dashed border-black/10 bg-white/72 px-4 py-6 text-sm leading-6 text-mute">还没有团队，先创建一个。</li>';
    renderTeamPanel();
    renderOverview();
    renderIcons();
    return;
  }

  for (const team of state.teams) {
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className =
      teamButtonClass + " " + (team.team_name === state.selectedTeam ? teamButtonActive : teamButtonIdle);
    button.innerHTML = `
      <div class="flex items-start justify-between gap-3">
        <div class="min-w-0">
          <span class="block truncate font-mono text-[13px] font-semibold">${team.team_name}</span>
          <span class="mt-1 block truncate text-sm text-mute">${team.display_name || "-"}</span>
        </div>
        <i data-lucide="chevron-right" class="mt-0.5 h-4 w-4 shrink-0 text-mute"></i>
      </div>
      <div class="mt-2 truncate text-xs text-mute">${team.git_remote || "no remote"}</div>
    `;
    button.onclick = () => selectTeam(team.team_name);
    li.appendChild(button);
    list.appendChild(li);
  }

  renderTeamPanel();
  renderOverview();
  renderIcons();
}

function renderTeamPanel() {
  const team = currentTeam();
  $("teamTitle").textContent = team ? team.team_name : "未选择团队";
  $("teamMeta").textContent = team
    ? `${team.display_name || "-"} | ${team.git_remote || "no remote"}`
    : "先在左侧选择团队。";
  $("remoteUrl").value = team?.git_remote || "";
  $("teamPassword").value = team ? (state.passwords[team.team_name] || "") : "";
}

function renderSecrets() {
  const list = $("secretList");
  list.innerHTML = "";
  if (!state.secrets.length) {
    list.innerHTML =
      '<li class="rounded-[22px] border border-dashed border-black/10 bg-white/72 px-4 py-6 text-sm leading-6 text-mute">输入团队密码后点击“加载 Keys”，或直接输入 key 读取 value。</li>';
    renderOverview();
    renderIcons();
    return;
  }

  for (const key of state.secrets) {
    const li = document.createElement("li");
    const button = document.createElement("button");
    button.type = "button";
    button.className =
      secretButtonClass + " " + (state.selectedKey === key ? secretButtonActive : secretButtonIdle);
    button.innerHTML = `
      <div class="flex items-center justify-between gap-3">
        <span class="secret-name block truncate font-mono text-[13px] font-semibold">${key}</span>
        <i data-lucide="key-square" class="h-4 w-4 shrink-0 text-mute"></i>
      </div>
    `;
    button.onclick = () => {
      state.selectedKey = key;
      $("secretKey").value = key;
      $("secretValue").value = "";
      renderSecrets();
    };
    li.appendChild(button);
    list.appendChild(li);
  }

  renderOverview();
  renderIcons();
}

function selectTeam(teamName) {
  state.selectedTeam = teamName;
  state.secrets = [];
  state.selectedKey = "";
  $("secretKey").value = "";
  $("secretValue").value = "";
  renderTeams();
  renderSecrets();
}

async function refreshTeams(keepSelection = true) {
  const data = await request("/api/teams");
  state.teams = data.teams || [];
  if (!keepSelection || !state.teams.some((team) => team.team_name === state.selectedTeam)) {
    state.selectedTeam = state.teams[0]?.team_name || null;
  }
  renderTeams();
  renderSecrets();
}

async function onCreateTeam() {
  const payload = {
    team: $("newTeamName").value.trim(),
    display_name: $("newTeamDisplayName").value.trim() || null,
    password: $("newTeamPassword").value,
    password_confirm: $("newTeamPasswordConfirm").value,
  };
  await request("/api/teams", {
    method: "POST",
    body: JSON.stringify(payload),
  });
  state.passwords[payload.team] = payload.password;
  $("newTeamName").value = "";
  $("newTeamDisplayName").value = "";
  $("newTeamPassword").value = "";
  $("newTeamPasswordConfirm").value = "";
  await refreshTeams(false);
  selectTeam(payload.team);
  setStatus("团队已创建");
}

async function onLoadSecrets() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  const data = await request(`/api/teams/${team.team_name}/secrets/list`, {
    method: "POST",
    body: JSON.stringify({ password }),
  });
  state.secrets = data.keys || [];
  renderSecrets();
  setStatus("keys 已加载");
}

async function onGetSecret() {
  const team = requireTeam();
  const key = $("secretKey").value.trim();
  if (!key) throw new Error("请输入 key");
  const data = await request(`/api/teams/${team.team_name}/secrets/get`, {
    method: "POST",
    body: JSON.stringify({ key }),
  });
  state.selectedKey = key;
  $("secretValue").value = data.value || "";
  renderSecrets();
  setStatus("读取成功");
}

async function onSaveSecret() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  const key = $("secretKey").value.trim();
  if (!key) throw new Error("请输入 key");
  await request(`/api/teams/${team.team_name}/secrets/set`, {
    method: "POST",
    body: JSON.stringify({ key, value: $("secretValue").value, password }),
  });
  state.selectedKey = key;
  await onLoadSecrets();
  setStatus("保存成功");
}

async function onDeleteSecret() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  const key = $("secretKey").value.trim();
  if (!key) throw new Error("请输入 key");
  await request(`/api/teams/${team.team_name}/secrets/delete`, {
    method: "POST",
    body: JSON.stringify({ key, password }),
  });
  $("secretKey").value = "";
  $("secretValue").value = "";
  state.selectedKey = "";
  await onLoadSecrets();
  setStatus("删除成功");
}

async function onSetRemote() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  await request(`/api/teams/${team.team_name}/remote`, {
    method: "POST",
    body: JSON.stringify({ url: $("remoteUrl").value.trim(), password }),
  });
  await refreshTeams();
  setStatus("remote 已更新");
}

async function onSyncTeam() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  await request(`/api/teams/${team.team_name}/sync`, {
    method: "POST",
    body: JSON.stringify({ password }),
  });
  setStatus("同步完成");
}

async function onDeleteTeam() {
  const team = requireTeam();
  const password = requirePassword(team.team_name);
  if (!confirm(`确认删除团队 ${team.team_name} ?`)) return;
  await request(`/api/teams/${team.team_name}/delete`, {
    method: "POST",
    body: JSON.stringify({ password }),
  });
  delete state.passwords[team.team_name];
  state.selectedTeam = null;
  await refreshTeams(false);
  setStatus("团队已删除");
}

async function onSyncAll() {
  await request("/api/sync-all", {
    method: "POST",
    body: JSON.stringify({ passwords: state.passwords }),
  });
  setStatus("全部团队同步完成");
}

function bindEvents() {
  $("refreshTeams").onclick = () => run(refreshTeams());
  $("syncAll").onclick = () => run(onSyncAll());
  $("createTeam").onclick = () => run(onCreateTeam());
  $("rememberPassword").onclick = () => {
    const team = currentTeam();
    if (!team) return;
    state.passwords[team.team_name] = $("teamPassword").value.trim();
    setStatus("密码已保存到当前会话");
  };
  $("loadSecrets").onclick = () => run(onLoadSecrets());
  $("getSecret").onclick = () => run(onGetSecret());
  $("saveSecret").onclick = () => run(onSaveSecret());
  $("deleteSecret").onclick = () => run(onDeleteSecret());
  $("setRemote").onclick = () => run(onSetRemote());
  $("syncTeam").onclick = () => run(onSyncTeam());
  $("deleteTeam").onclick = () => run(onDeleteTeam());
  $("clearSecret").onclick = () => {
    $("secretKey").value = "";
    $("secretValue").value = "";
    state.selectedKey = "";
    renderSecrets();
  };
}

async function run(task) {
  setStatus("处理中...");
  try {
    await task;
  } catch (error) {
    setStatus(error.message || "操作失败", true);
    return;
  }
  if ($("status").textContent === "处理中...") {
    setStatus("");
  }
}

bindEvents();
renderIcons();
run(refreshTeams(false));
"##;
