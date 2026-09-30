# Rovar 服务器

[English](README.md) | 简体中文

Salvo + SQLx + PostgreSQL，内嵌 Web 编辑器。文档和组件快照加密落盘，服务器持有解密密钥。

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

## 账号与同步恢复

从空间菜单打开「账号设置」，输入当前密码即可修改密码，也可以查看和撤销已登录的会话。修改密码会退出其他会话，当前编辑器保持连接。会话标识与登录令牌分离。

桌面端可保存服务器名称、切换多个账号。服务器列表支持重命名；移除前需要先同步待上传修改、关闭其文档并退出账号。移除地址保留文档缓存，方便以后重新连接。

编辑内容先保存到本地再上传。失败上传每 30 秒重试，也可在云同步状态面板立即重试。待上传请求可跨重启恢复，先使用原幂等键重放，再上传之后的修改。会话过期后暂停上传，通过「重新登录」使用原账号恢复，无需关闭文档。「保留双方版本」将本地内容另存为新文档，并单独取回服务器版本。

账号接口（需要登录）：`PUT /api/v1/account/password`、`GET /api/v1/account/sessions`、`DELETE /api/v1/account/sessions/{id}`，以及 `DELETE /api/v1/account/sessions`（仅退出其他会话）。

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
