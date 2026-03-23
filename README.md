# rupass

轻量级团队密码管理工具。

详细用法看：

```bash
rupass --help
```

也支持完整 CLI，方便脚本或 AI 直接通过命令行操作。

## 规则

- 团队名必须以 `_team` 结尾。
- 每个团队有自己的独立配置(密钥，远程git地址等)。
- 不传团队时：
  - 如果本地没有团队，需要先运行 `rupass tui` 创建团队
  - 如果本地只有一个团队，默认使用它
  - 如果本地有多个团队，必须显式传团队
- 新机器导入已有团队时，运行 `rupass team import <remote>`

## 存储目录

```text
~/.rupass/
├── config/
│   └── your_team.sec
└── store/
    └── your_team/
```

## 开发命令

```bash
pnpm dev
pnpm check
pnpm build
pnpm test
pnpm fmt
pnpm clippy
```

## CLI 示例

```bash
# 通用命令
rupass tui
rupass team list
rupass team create my_team --password secret
rupass team import git@github.com:org/repo.git --password secret
rupass team set-remote my_team git@github.com:org/repo.git
rupass sync-all
```

## 新机器导入

如果远程仓库已经由旧机器同步过最新的 team 元数据，可直接导入：

```bash
rupass team import git@github.com:org/repo.git --password secret
```

导入后即可继续同步：

```bash
rupass team sync my_team --password secret
# 或
rupass sync-all
```

### 默认团队示例

仅当本地只有一个团队时可省略团队名：

```bash
rupass list
rupass get db_password
rupass set db_password 'hello123'
rupass del db_password
```

### 传递团队示例

显式传入团队名：

```bash
rupass my_team list
rupass my_team get db_password
rupass my_team set db_password 'hello123'
rupass my_team del db_password
rupass team del my_team --password secret
rupass team clear-remote my_team --password secret
rupass team sync my_team --password secret
```

## 安装 release

编译并安装当前系统的 release 二进制：

```bash
pnpm install:release
```

默认安装到：

- macOS / Linux: `~/.local/bin/rupass`
- Windows: `~/.local/bin/rupass.exe`
