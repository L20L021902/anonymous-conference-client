# 匿名会议系统客户端

\>[可执行文件可在发布页面获取](https://github.com/L20L021902/anonymous-conference-client/releases/latest)<

## 运行方式
`RUST_LOG=debug cargo run`

或

`RUST_LOG=debug cargo run -- <可选的命令行参数>`

或

直接运行可执行文件(anonymous-conference-client.exe或anonymous-conference-client)

>`RUST_LOG=debug`将日志级别设置为调试

可选的命令行参数：

| 参数 | 说明 | 实例 |
| ----------- | ----------- | ----------- |
| `--cli` | 以cli模式运行应用程序前端 | |
| `--server-address <服务器的地址>` | 设置服务器地址（默认为 `localhost:7667`）| `--server-address 127.0.0.1:6666` |

---

## 编译方式
`cargo build`

---

## cli模式运行应用程序前端的命令
 
| 命令 | 说明 | 实例 |
| ----------- | ----------- | ----------- |
|`/create <会议密码>`| 使用提供的密码创建会议 | `/create hello` |
|`/join <会议ID> <会议密码>`| 使用提供的ID和密码加入会议 | `/join 8845684583 hello` |
|`/leave`| 离开当前会议 | `/leave` |
|`<其它输入>`| 用提供的文本向当前会议发送消息 | `你好` |

