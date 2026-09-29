# Rovar 服务器

[English](README.md) | 简体中文

Salvo + SQLx + PostgreSQL，内嵌 Web 编辑器。文档和组件快照加密落盘，服务器持有解密密钥。

## 运行

在仓库根目录使用 Nushell：

```nu
nu scripts/server.nu
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

## 空间

通过工作区菜单切换个人和团队空间。文档、组件、版本及客户端缓存按空间隔离。桌面端还支持多个服务器和完全本地使用。

团队所有者可生成邀请码、移除成员；成员可自行退出。邀请码单次使用、7 天有效，重新生成将替换旧码。加入已有团队不受注册开关限制。

备份需同时包含 PostgreSQL 和存储目录，包括 `master.key`。桌面凭据使用系统凭据存储，Web 会话使用 HttpOnly Cookie。客户端缓存不加密。

## 结构

- `domain/`：模型、校验和错误。
- `application/`：认证、空间与文档用例，以及仓储和存储接口。
- `infrastructure/`：PostgreSQL 事务、密码哈希和加密存储。
- `http/`：Salvo 请求处理、认证和 DTO 转换。
- `bootstrap/`：TOML 配置与依赖组装。

DTO 位于 `rovar-api`。保存使用基础版本和幂等请求标识，冲突时保留本地副本。单份快照上限 128 MiB。

## 测试

```nu
cargo test -p rovar-server
$env.ROVAR_TEST_DATABASE_URL = "postgresql://user:password@localhost/disposable_test_db"
cargo test -p rovar-server -- --include-ignored
```

集成测试使用一次性数据库。
