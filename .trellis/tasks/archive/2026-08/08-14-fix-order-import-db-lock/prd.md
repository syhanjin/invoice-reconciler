# 修复订单并发导入数据库锁定

## Goal

修复已有订单识别任务运行时再次导入一批截图后，SQLite 写锁竞争和连接池耗尽导致应用无响应、订单列表与任务池暂时全部不可见的问题。重启后现有数据能够恢复，说明本任务必须消除运行时数据库阻塞并保留全部持久化数据，而不是引入数据重建逻辑。

## Requirements

- 将应用 SQLite 连接池固定为单连接；订单识别和摘要生成的网络请求仍按现有配置并发，只串行化短暂的本地数据库操作。
- 使用 SeaORM 的 SQLite 连接选项对每个连接启用外键约束并设置明确的 busy timeout，不依赖连接建立后对池中某个随机连接执行 `PRAGMA foreign_keys`。
- 第二批订单图片继续追加到同一个持久化任务池；不得恢复订单批次概念或禁止并发导入。
- 保持订单池状态机、成功完成原子事务、不可变完成日志、图片所有权、启动恢复和所有公共 Tauri 接口不变。
- 在数据库规范和项目级 `AGENTS.md` 中记录 SQLite 单连接策略，避免未来通过增加数据库连接数实现识别并发。

## Acceptance Criteria

- [x] 8 个订单完成事务、第二批任务创建以及订单/任务读取并发执行时，在限定时间内全部完成且不返回 `database is locked`。
- [x] 并发前的历史订单保留；每个成功任务只创建一个订单和一条完成日志；第二批任务存在于池中或正常完成。
- [x] 关闭并重新连接同一数据库后，订单、池任务和完成日志仍完整可读。
- [x] 生产文件数据库的 SQLite 连接启用外键约束并采用约定的 busy timeout。
- [x] 严格 Clippy、全部 Rust 测试、前端 lint/test/build 全部通过。

## Out of Scope

- 数据库表结构或迁移。
- Tauri API、前端类型、事件或界面交互变更。
- 多个应用进程同时写入同一个 `db.sqlite3`。

## Technical Notes

- 当前生产池 `max_connections(5)`，订单识别并发最大为 8。多个 `persist_pool_order` 事务会先读后写并同时占用池连接，造成 SQLite 写锁升级竞争和连接池饥饿。
- 订单页和任务页在重新挂载时从空投影开始加载；数据库调用被阻塞时会表现为两处数据同时消失。重启释放连接并执行现有 `processing -> failed` 恢复，因此持久化数据重新出现。
- 相关规范：`.trellis/spec/backend/database-guidelines.md`、`.trellis/spec/backend/order-import-contract.md`。
