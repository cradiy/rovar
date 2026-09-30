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

## 备份

同时备份 PostgreSQL 和存储目录，包括 `master.key`。解密已存储的文档需要此密钥。
