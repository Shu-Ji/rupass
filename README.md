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
  - 如果本地没有团队，会自动创建 `default_team`
  - 如果本地只有一个团队，默认使用它
  - 如果本地有多个团队，必须显式传团队

## 存储目录

```text
~/.rupass/
├── config/
│   └── default_team.sec
└── store/
    └── default_team/
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


我同意，当前这版纯手写单页不适合继续补了。现在我改方向：上 React + TypeScript + Ant Design + Tailwind + Lucide + Rsbuild，把
  UI 拆成真正的团队管理台，再把前端编译产物内嵌进 Rust 一起发布。
