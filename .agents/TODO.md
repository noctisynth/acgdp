# acgdp TODO 索引

> 状态：Active
>
> 作用：实施清单入口；不在本文件重复具体任务
>
> 最后核对：2026-09-29

清单只记录 [设计索引](DESIGN.md) 指向的 Active Spec 与当前实现之间的差异，不是独立需求来源。新性能方案先确定兼容性与验收标准，再写入 Spec 和清单。

## 实施清单

| 范围 | 清单 | 状态 | 当前重点 |
| --- | --- | --- | --- |
| 递归解压 | [todos/extraction.md](todos/extraction.md) | Active | 真实大包、分卷与加密 RAR 验证 |
| 项目与发布 | [todos/project.md](todos/project.md) | Active | crates.io 首次发布、多平台 Release 资产 |

## 使用规则

- 先从 [设计索引](DESIGN.md) 找到对应 Active Spec，再进入实施清单。
- 已完成项必须有实现和必要验证；未完成项保留未勾选，并说明缺少的证据或依赖。
- 跨格式与发布边界的任务记入项目清单，并链接相关 Spec。
