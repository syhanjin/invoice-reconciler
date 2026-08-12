# 独立导入任务中心与持久化历史

## Goal

将订单页中的临时展开任务面板迁移为独立的导入任务中心，展示 SQLite 中持久化的全部订单导入批次，并改善任务列表背景、滚动与长期历史浏览体验。

## Requirements

- 左侧一级导航为“订单 / 导入任务 / 设置”；发票一级页面本次不实现。
- 导入任务页内部使用“订单 / 发票”两个 tab；订单默认选中，发票显示但禁用。
- 订单页移除批次展开面板，保留订单筛选、表格及截图导入。
- 截图批次创建成功后更新活动批次状态，导航到导入任务页并选中新批次；取消选择或失败时留在订单页。
- 订单任务按持久化批次展示，采用分页批次列表与选中批次详情布局。
- 历史批次默认每页 20 条，可选 20、50、100；切换页大小后回到第一页。
- 批次详情继续支持重试失败项、重试单项、取消排队批次与取消单项。
- 实时任务事件更新活动批次及任务页中匹配的批次，不覆盖用户查看的无关批次。
- 任务区域使用不透明背景、稳定尺寸和清晰的滚动条交互。

## Acceptance Criteria

- [ ] 左侧可进入独立“导入任务”页面，订单页不再显示展开任务面板。
- [ ] 任务页默认显示可用的“订单”tab和禁用的“发票”tab。
- [ ] 应用重启后仍能分页浏览既有订单导入批次。
- [ ] 批次列表按 `created_at DESC, id DESC` 稳定排序，并显示正确的状态计数。
- [ ] 选择批次时延迟加载完整任务详情；空列表、加载和请求失败状态明确且不显示过期内容。
- [ ] 创建批次成功后自动进入任务页并选中新批次。
- [ ] 页大小支持 20、50、100，切换时重置到第一页。
- [ ] 重试、取消及实时任务事件后，所选详情和历史摘要保持一致。
- [ ] 任务历史和详情均有实色背景；长列表可顺畅滚动，滚动条在静止、悬停与拖动时清晰。
- [ ] 默认窗口和最小窗口下无文本遮挡、布局跳动或不可访问操作。
- [ ] Rust 测试、前端构建、lint 与测试全部通过。

## Definition of Done

- 后端分页接口、前端任务中心、导航与导入跳转均实现。
- 新增数据库分页/聚合测试及关键前端交互测试。
- `cargo test --manifest-path src-tauri/Cargo.toml`、`bun run build`、`bun run lint`、`bun run test` 通过。
- 评估并更新适用的 Trellis 规范，按项目约定创建范围明确的提交并完成 finish-work。

## Technical Approach

- 复用现有 `import_batches`、`import_tasks` 数据表，无需迁移。
- 新增 `list_import_batches` Tauri 命令和 `ImportBatchesQuery`、`ImportBatchSummary`、`ImportBatchesPage` 类型。摘要接口按批次聚合任务计数，不返回任务明细；详情复用 `get_batch`。
- 查询页码最小为 1，页大小后端限制在 `1..=100`，前端只暴露 20、50、100。
- 新增任务页面组件，在应用层保留待选批次 ID并传入页面；从订单页导入成功时设置该 ID并切换导航。
- 任务事件继续使用完整 `BatchDetail`。当前查看批次匹配时直接更新详情，并刷新或合并当前页摘要。
- 页面采用批次列表与详情双栏工作台；窄窗口保持可操作的最小宽度。滚动条改善通过现有 shadcn `ScrollArea` 完成，并回归检查订单编辑抽屉。

## Decision (ADR-lite)

**Context**: 导入任务需要跨重启访问，并为未来订单与发票两类导入提供统一入口。

**Decision**: 使用独立一级任务中心，内部按业务类型分 tab；当前实现订单任务，保留禁用的发票 tab。历史摘要分页加载，批次任务按需加载。

**Consequences**: 本次需要新增跨 Rust/Tauri/TypeScript 的分页接口，但不改变数据库结构。发票只保留信息架构入口，不提前引入发票领域模型。

## Out of Scope

- 发票页面、发票导入流程及发票任务后端模型。
- 删除批次或任务，以及关联订单和托管图片的级联清理。
- 状态/日期筛选、搜索、无限滚动和批量删除。
- 数据库迁移或新的持久化技术。

## Technical Notes

- 相关前端入口：`src/App.tsx`、`src/components/app-shell.tsx`、`src/components/orders-page.tsx`、`src/components/batch-panel.tsx`。
- 相关后端入口：`src-tauri/src/models.rs`、`src-tauri/src/db.rs`、`src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`。
- 必须遵循 `.trellis/spec/frontend/application-contract.md`；所有 Tauri 调用继续集中在 `src/lib/api.ts`，公开字段使用 camelCase。
