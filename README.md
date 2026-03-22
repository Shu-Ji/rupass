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
rupass team create dev_team --password secret
rupass team set-remote dev_team git@github.com:org/repo.git
rupass sync-all
```

### 默认团队示例

仅当本地只有一个团队时可省略团队名：

```bash
rupass get db_password
rupass key list
rupass key get db_password
rupass key set db_password 'hello123'
rupass key delete db_password
```

### 传递团队示例

显式传入团队名：

```bash
rupass this_is_a_test_team get db_password
rupass key list --team this_is_a_test_team
rupass key get --team this_is_a_test_team db_password
rupass key set --team this_is_a_test_team db_password 'hello123'
rupass key delete --team this_is_a_test_team db_password
rupass team delete this_is_a_test_team --password secret
rupass team clear-remote this_is_a_test_team --password secret
rupass team sync this_is_a_test_team --password secret
```

## 安装 release

编译并安装当前系统的 release 二进制：

```bash
pnpm install:release
```

默认安装到：

- macOS / Linux: `~/.local/bin/rupass`
- Windows: `~/.local/bin/rupass.exe`
