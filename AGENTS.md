# CLAUDE.md

ImplicitContractDefense.md 是隐式契约防御的介绍，ai coding代码实现可以参考这个文档。

## 项目结构

Monorepo，两个子目录：
- `server/` — Rust 后端（Axum + SQLite）
- `frontend/` — Next.js 前端（TypeScript + shadcn/ui）

## TDD开发流程
1. 先写测试（按功能模块粒度，不是单函数），跑 `cargo test` 确认编译通过但断言失败
2. 实现代码，跑 `cargo test` 确认绿灯
3. 需要时重构，保持绿灯
4. 测试必须覆盖实际代码路径，不是理想调用方式

## 跨语言类型安全（前后端契约）

后端和前端通过 ts-rs 自动生成类型对齐，规则如下：

### 后端规则
- 所有带 `#[ts(export)]` 的 DTO **只能定义在 `server/src/api/dto.rs`** 中
- 其他文件禁止使用 `#[ts(export)]`
- `cargo test` 会自动生成 `frontend/src/api/types.generated.ts`

### 前端规则
- 与后端 API 对接的类型 **只能从 `src/api/types.generated.ts` 导入**
- `types.generated.ts` 是自动生成文件，**禁止手动修改**
- 前端 `src/api/` 目录下禁止自行定义与后端重复的类型

### 验证
改了后端 DTO 或前端 API 类型后：
1. `cargo test`（触发 ts-rs 重新生成）
2. 前端 `npx tsc --noEmit`（TypeScript 编译检查）
3. `bash scripts/check-contracts.sh`（契约检测）

## 改完代码必须验证
改完代码跑一次质量门禁：
```bash
bash scripts/check.sh
```
check.sh = clippy + cargo test + cargo build + magic value scan + 契约检测。
TDD 循环内用 `cargo test` 快速迭代，最终提交前跑 check.sh 全量检查。

## 提交代码
验证通过后，commit 并 push

## 实施约束
- 不做兼容：旧代码旧结构直接删，不保留向后兼容逻辑
- 必须删旧代码：如果新设计不需要某个字段/函数/文件，直接删除
