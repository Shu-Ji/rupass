pub(crate) const INDEX_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>rupass ui</title>
  <script src="https://cdn.tailwindcss.com"></script>
  <script>
    tailwind.config = {
      theme: {
        extend: {
          colors: {
            paper: "#f5f3ef",
            ink: "#161616",
            mute: "#6b6b6b",
            line: "#dfdbd2",
            panel: "rgba(255,255,255,0.76)",
            accent: "#1f3a5f",
            accentSoft: "#eef2f7",
            danger: "#9f2f2f"
          },
          boxShadow: {
            panel: "0 24px 60px rgba(17, 24, 39, 0.08)"
          },
          fontFamily: {
            sans: ["Manrope", "PingFang SC", "Noto Sans SC", "sans-serif"],
            mono: ["IBM Plex Mono", "SFMono-Regular", "monospace"]
          }
        }
      }
    };
  </script>
  <script src="https://unpkg.com/lucide@latest"></script>
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin />
  <link href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600&family=Manrope:wght@400;500;600;700;800&display=swap" rel="stylesheet" />
  <style>
    .skip-link {
      position: absolute;
      left: 16px;
      top: -48px;
      z-index: 50;
      border-radius: 999px;
      background: #161616;
      padding: 10px 14px;
      color: #fff;
      text-decoration: none;
    }
    .skip-link:focus { top: 16px; }
    ::selection {
      background: rgba(31, 58, 95, 0.14);
    }
    ::-webkit-scrollbar { width: 10px; height: 10px; }
    ::-webkit-scrollbar-thumb {
      border-radius: 999px;
      background: rgba(22, 22, 22, 0.18);
    }
  </style>
</head>
<body class="min-h-screen bg-paper text-ink">
  <a class="skip-link focus:outline-none focus:ring-4 focus:ring-black/10" href="#main">跳到主内容</a>

  <div class="mx-auto max-w-[1480px] px-4 py-4 md:px-7 md:py-7">
    <header class="mb-5 rounded-[32px] border border-black/5 bg-panel px-6 py-6 shadow-panel backdrop-blur-xl md:px-8 md:py-8">
      <div class="grid gap-8 xl:grid-cols-[1.35fr_0.9fr] xl:items-end">
        <div class="space-y-5">
          <div class="inline-flex items-center gap-2 rounded-full border border-black/8 bg-white/70 px-3 py-1.5 text-[11px] font-semibold uppercase tracking-[0.28em] text-mute">
            <i data-lucide="shield" class="h-3.5 w-3.5"></i>
            Local Secret Console
          </div>

          <div class="max-w-4xl">
            <h1 class="text-3xl font-semibold tracking-[-0.04em] md:text-5xl md:leading-[1.02]">
              一个更克制的
              <span class="text-accent">rupass</span>
              管理界面
            </h1>
            <p class="mt-4 max-w-3xl text-sm leading-7 text-mute md:text-[15px]">
              只保留团队、密钥和同步三个核心维度。没有多余装饰，没有复杂层级，重点是稳定、清晰、可直接操作。
            </p>
          </div>
        </div>

        <div class="grid gap-3 md:grid-cols-3 xl:grid-cols-3">
          <div class="rounded-[24px] border border-black/6 bg-white/72 px-4 py-4">
            <div class="text-[11px] font-semibold uppercase tracking-[0.22em] text-mute">Teams</div>
            <div id="teamCount" class="mt-4 font-mono text-2xl font-semibold">0</div>
          </div>
          <div class="rounded-[24px] border border-black/6 bg-white/72 px-4 py-4">
            <div class="text-[11px] font-semibold uppercase tracking-[0.22em] text-mute">Keys</div>
            <div id="keyCount" class="mt-4 font-mono text-2xl font-semibold">0</div>
          </div>
          <div class="rounded-[24px] border border-black/6 bg-white/72 px-4 py-4">
            <div class="text-[11px] font-semibold uppercase tracking-[0.22em] text-mute">Current</div>
            <div id="activeTeamBadge" class="mt-4 truncate font-mono text-sm font-semibold">none</div>
          </div>
        </div>
      </div>

      <div class="mt-6 grid gap-3 lg:grid-cols-[1fr_auto] lg:items-center">
        <div class="flex flex-wrap gap-2">
          <span class="rounded-full border border-black/8 bg-white/70 px-3 py-1.5 text-xs font-medium text-mute">`get` 默认免密码</span>
          <span class="rounded-full border border-black/8 bg-white/70 px-3 py-1.5 text-xs font-medium text-mute">其他团队操作按团队验密</span>
          <span class="rounded-full border border-black/8 bg-white/70 px-3 py-1.5 text-xs font-medium text-mute">`sync-all` 逐个同步所有团队</span>
        </div>
        <div id="status" aria-live="polite" class="min-h-11 rounded-[20px] border border-black/6 bg-white/75 px-4 py-3 text-sm font-medium text-mute"></div>
      </div>
    </header>

    <div class="grid gap-5 xl:grid-cols-[340px_minmax(0,1fr)]">
      <aside class="rounded-[32px] border border-black/5 bg-panel shadow-panel backdrop-blur-xl">
        <div class="space-y-6 p-5">
          <div class="flex items-start justify-between gap-3">
            <div>
              <div class="flex items-center gap-2 text-lg font-semibold tracking-[-0.02em]">
                <i data-lucide="folders" class="h-5 w-5 text-accent"></i>
                Teams
              </div>
              <p class="mt-1 text-sm text-mute">切换、创建、删除与同步团队。</p>
            </div>
            <div class="flex gap-2">
              <button id="refreshTeams" type="button" class="inline-flex items-center gap-2 rounded-full border border-black/8 bg-white/78 px-3 py-2 text-sm font-medium text-ink transition hover:bg-white focus:outline-none focus:ring-4 focus:ring-black/5">
                <i data-lucide="refresh-cw" class="h-4 w-4"></i>
                刷新
              </button>
              <button id="syncAll" type="button" class="inline-flex items-center gap-2 rounded-full bg-ink px-3 py-2 text-sm font-medium text-white transition hover:opacity-92 focus:outline-none focus:ring-4 focus:ring-black/10">
                <i data-lucide="workflow" class="h-4 w-4"></i>
                同步全部
              </button>
            </div>
          </div>

          <ul id="teamList" class="grid gap-2"></ul>

          <section class="rounded-[26px] border border-black/6 bg-white/66 p-4">
            <div class="flex items-center gap-2 text-base font-semibold tracking-[-0.02em]">
              <i data-lucide="plus" class="h-4.5 w-4.5 text-accent"></i>
              创建团队
            </div>
            <p class="mt-1 text-sm text-mute">尽量保持命名稳定、显示名克制。</p>

            <div class="mt-4 space-y-3">
              <div>
                <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="newTeamName">团队名</label>
                <input id="newTeamName" autocomplete="off" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="this_is_a_test_team" />
              </div>
              <div>
                <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="newTeamDisplayName">显示名</label>
                <input id="newTeamDisplayName" autocomplete="off" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="显示名" />
              </div>
              <div class="grid gap-3 md:grid-cols-2 xl:grid-cols-1 2xl:grid-cols-2">
                <div>
                  <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="newTeamPassword">密码</label>
                  <input id="newTeamPassword" type="password" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="密码" />
                </div>
                <div>
                  <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="newTeamPasswordConfirm">确认密码</label>
                  <input id="newTeamPasswordConfirm" type="password" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="确认密码" />
                </div>
              </div>
              <button id="createTeam" type="button" class="inline-flex w-full items-center justify-center gap-2 rounded-[18px] bg-accent px-4 py-3 font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-accent/15">
                <i data-lucide="folder-plus" class="h-4 w-4"></i>
                创建团队
              </button>
            </div>
          </section>
        </div>
      </aside>

      <main id="main" class="space-y-5">
        <section class="rounded-[32px] border border-black/5 bg-panel p-5 shadow-panel backdrop-blur-xl">
          <div class="grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
            <div class="space-y-5">
              <div>
                <div class="flex items-center gap-2 text-xl font-semibold tracking-[-0.03em]">
                  <i data-lucide="shield-check" class="h-5 w-5 text-accent"></i>
                  <span id="teamTitle">未选择团队</span>
                </div>
                <div id="teamMeta" class="mt-2 text-sm text-mute">先在左侧选择团队。</div>
              </div>

              <div class="grid gap-4 xl:grid-cols-2">
                <div>
                  <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="teamPassword">团队密码</label>
                  <input id="teamPassword" type="password" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="get 以外操作需要" />
                </div>
                <div>
                  <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="remoteUrl">Git Remote</label>
                  <input id="remoteUrl" type="url" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="git@github.com:org/repo.git" />
                </div>
              </div>
            </div>

            <div class="rounded-[26px] border border-black/6 bg-white/66 p-4">
              <div class="text-[11px] font-semibold uppercase tracking-[0.22em] text-mute">Actions</div>
              <div class="mt-4 grid gap-2">
                <button id="rememberPassword" type="button" class="inline-flex items-center justify-center gap-2 rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 text-sm font-medium text-ink transition hover:bg-white focus:outline-none focus:ring-4 focus:ring-black/5">
                  <i data-lucide="key-round" class="h-4 w-4"></i>
                  保存密码到当前会话
                </button>
                <button id="loadSecrets" type="button" class="inline-flex items-center justify-center gap-2 rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 text-sm font-medium text-ink transition hover:bg-white focus:outline-none focus:ring-4 focus:ring-black/5">
                  <i data-lucide="database" class="h-4 w-4"></i>
                  加载 Keys
                </button>
                <button id="setRemote" type="button" class="inline-flex items-center justify-center gap-2 rounded-[18px] bg-accent px-4 py-3 text-sm font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-accent/15">
                  <i data-lucide="git-branch-plus" class="h-4 w-4"></i>
                  设置 Remote
                </button>
                <button id="syncTeam" type="button" class="inline-flex items-center justify-center gap-2 rounded-[18px] bg-ink px-4 py-3 text-sm font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-black/10">
                  <i data-lucide="refresh-ccw" class="h-4 w-4"></i>
                  同步团队
                </button>
                <button id="deleteTeam" type="button" class="inline-flex items-center justify-center gap-2 rounded-[18px] bg-danger px-4 py-3 text-sm font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-red-200/40">
                  <i data-lucide="folder-minus" class="h-4 w-4"></i>
                  删除团队
                </button>
              </div>
            </div>
          </div>
        </section>

        <section class="grid gap-5 xl:grid-cols-[320px_minmax(0,1fr)]">
          <section class="rounded-[32px] border border-black/5 bg-panel p-5 shadow-panel backdrop-blur-xl">
            <div class="mb-4">
              <div class="flex items-center gap-2 text-lg font-semibold tracking-[-0.02em]">
                <i data-lucide="file-key-2" class="h-5 w-5 text-accent"></i>
                Keys
              </div>
              <p class="mt-1 text-sm text-mute">先加载 Keys，再从左侧选择，或直接输入 key。</p>
            </div>
            <ul id="secretList" class="grid gap-2"></ul>
          </section>

          <section class="rounded-[32px] border border-black/5 bg-panel p-5 shadow-panel backdrop-blur-xl">
            <div class="space-y-4">
              <div class="grid gap-4 xl:grid-cols-[minmax(0,1fr)_auto]">
                <div>
                  <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="secretKey">Key</label>
                  <input id="secretKey" autocomplete="off" class="w-full rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 font-mono text-[13px] outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="db_password" />
                </div>
                <div class="flex flex-wrap items-end gap-2">
                  <button id="getSecret" type="button" class="inline-flex items-center gap-2 rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 text-sm font-medium text-ink transition hover:bg-white focus:outline-none focus:ring-4 focus:ring-black/5">
                    <i data-lucide="search" class="h-4 w-4"></i>
                    读取
                  </button>
                  <button id="deleteSecret" type="button" class="inline-flex items-center gap-2 rounded-[18px] bg-danger px-4 py-3 text-sm font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-red-200/40">
                    <i data-lucide="trash-2" class="h-4 w-4"></i>
                    删除
                  </button>
                </div>
              </div>

              <div>
                <label class="mb-1.5 block text-[11px] font-semibold uppercase tracking-[0.2em] text-mute" for="secretValue">Value</label>
                <textarea id="secretValue" class="min-h-[360px] w-full rounded-[24px] border border-black/8 bg-white/90 px-4 py-4 font-mono text-[13px] leading-7 outline-none transition focus:border-black/15 focus:ring-4 focus:ring-black/5" placeholder="secret value"></textarea>
              </div>

              <div class="flex flex-wrap gap-2.5">
                <button id="saveSecret" type="button" class="inline-flex items-center gap-2 rounded-[18px] bg-accent px-4 py-3 font-medium text-white transition hover:opacity-95 focus:outline-none focus:ring-4 focus:ring-accent/15">
                  <i data-lucide="save" class="h-4 w-4"></i>
                  保存
                </button>
                <button id="clearSecret" type="button" class="inline-flex items-center gap-2 rounded-[18px] border border-black/8 bg-white/90 px-4 py-3 font-medium text-ink transition hover:bg-white focus:outline-none focus:ring-4 focus:ring-black/5">
                  <i data-lucide="eraser" class="h-4 w-4"></i>
                  清空
                </button>
              </div>
            </div>
          </section>
        </section>
      </main>
    </div>
  </div>

  <script src="/ui.js"></script>
</body>
</html>
"##;
