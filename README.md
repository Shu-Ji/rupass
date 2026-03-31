# rupass

轻量级团队密码管理工具。

详细用法看：

```bash
rupass --help
```

也支持完整 CLI，方便脚本或 AI 直接通过命令行操作。

## 规则

- 团队名必须以 `_team` 结尾。
- 每个团队有一个可分发的 `public/<team>.json`，以及一个仅本地保存的 `privite/<team>.key`。
- 不传团队时：
  - 如果本地没有团队，需要先运行 `rupass tui` 创建团队
  - 如果本地只有一个团队，默认使用它
  - 如果本地有多个团队，必须显式传团队

## 存储目录

```text
~/.rupass/
├── privite/
│   └── your_team.key
└── public/
    └── your_team.json
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
rupass team import-file ./finn_team.json --password secret
rupass team del my_team --password secret
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
```

## 安装 release

编译并安装当前系统的 release 二进制：

```bash
pnpm install:release
```

默认安装到：

- macOS / Linux: `~/.local/bin/rupass`
- Windows: `~/.local/bin/rupass.exe`
