# rupass

轻量级团队密码管理工具。

详细用法看：

```bash
rupass --help
```

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

## 安装 release

编译并安装当前系统的 release 二进制：

```bash
pnpm install:release
```

默认安装到：

- macOS / Linux: `~/.local/bin/rupass`
- Windows: `~/.local/bin/rupass.exe`
