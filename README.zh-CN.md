# 蜉邮（Mayfile）

生于瞬息，死于阅后。

一个使用 Rust 构建的极简、高性能、自托管的代码粘贴板（Pastebin）服务。

## 功能特性

- 🚀 **高性能**: 基于 Rust 和 Axum 构建，速度极快且资源占用低。
- 💾 **简单存储**: 使用 SQLite，部署简单，无需配置复杂的数据库服务。
- 🌐 **国际化支持**: 根据浏览器请求头自动切换中英文界面。
- ⏳ **过期控制**: 支持配置代码片段的过期时间。
- 🎨 **语法高亮**: 支持 Rust, Python, JavaScript, Go 等多种编程语言的语法高亮。
- 🔒 **灵活配置**: 可自定义 Token 长度、内容大小限制等。
- 🐳 **Docker 支持**: 提供 Docker 镜像和 Docker Compose 配置，一键部署。

## 快速开始

注意：为了安全起见，配置文件和 `docker-compose.yml` 已被 git 忽略。您需要使用提供的示例文件创建它们。

### 使用 Docker (推荐)

1. 克隆代码仓库:
   ```bash
   git clone https://github.com/Licheam/mayfile.git
   cd mayfile
   ```

2. **准备配置文件**:
   ```bash
   cp docker-compose.yml.example docker-compose.yml
   cp config/app.toml.example config/app.toml
   ```

3. 启动服务:
   ```bash
   docker-compose up -d
   ```

服务启动后访问 `http://localhost:3000` 即可使用。

### 手动安装

前置要求:
- Rust (最新稳定版)
- SQLite

1. **准备配置文件**:
   ```bash
   cp config/app.toml.example config/app.toml
   ```

2. 运行程序:
   ```bash
   cargo run --release
   ```

## 配置说明

所有配置均在 `config/app.toml` 文件中进行管理。请确保先将 `config/app.toml.example` 复制为 `config/app.toml`。您可以根据需要修改：

- **Server**: 监听地址和端口。
- **Paste**: 数据库路径、默认过期时间、Token 长度选项、最大内容限制等。
- **I18n**: 语言包路径。

示例配置 (`config/app.toml`):

```toml
[server]
host = "0.0.0.0"
port = 3000

[paste]
db_path = "data/pastebin.db"
default_expires_secs = 86400  # 默认过期时间 1 天
max_content_length = 1000000  # 最大内容 1 MB
```

## API 接口

- `GET /`: 首页。
- `POST /paste`: 上传新的代码片段。
- `GET /p/{token}`: 查看代码片段。
- `GET /r/{token}`: 查看代码片段原始内容 (Raw)。

## 许可证

MIT License
