# Rovar 服务器

[English](README.md) | 简体中文

提供 Web 编辑器、账号、个人与团队空间及文档同步，需要 PostgreSQL。文档加密存储，密钥由服务器持有。

## 运行

安装 just 和 Nushell 后，在仓库根目录执行：

```nu
just build-server
cp dist/rovar-server.example.toml rovar-server.toml
# 设置 PostgreSQL 地址、公开访问源地址和存储目录。
./dist/rovar-server --config rovar-server.toml
```

存储路径相对于配置文件。远程访问使用 HTTPS 反向代理，并将 `server.public_origin` 设置为浏览器访问的源地址。

`registration.personal` 控制个人注册；`registration.teams` 控制团队注册及已有账号创建团队。两项默认均为 `false`。团队注册会同时创建账号、个人空间和团队。

管理员可以不受注册开关限制，创建个人账号：

```nu
(input --suppress-output "密码：") | ./dist/rovar-server --config rovar-server.toml --create-user cradiy
```

密码至少 6 个字符。

`storage.retention` 默认保留最近 `history_days`（30 天）的历史内容，以及每个文档最新
`history_versions`（100 个）版本；同时超出这两项范围的内容才会过期。当前版本和上传确认记录
始终保留；增量基线过期的客户端会收到完整快照。无引用媒体和孤立文件有 `orphan_days`
（默认 7 天）缓冲期。三项参数都必须为正数，传输进行中会暂停回收。

## 备份

同时备份 PostgreSQL 和存储目录，包括 `master.key`。解密已存储的文档需要此密钥。
