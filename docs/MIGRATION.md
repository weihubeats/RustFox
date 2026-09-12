# 从 Apifox / Postman 迁移到 RustFox

> 英文版见 [en/MIGRATION.md](en/MIGRATION.md)。
> RustFox 以**开放格式**为迁移桥梁：只要源工具能导出 OpenAPI / Postman Collection，
> 就能整体搬入；搬不进来的（历史、登录态、 runner 配置）文末有手动对照。

## 1. 一览：什么能自动转，什么要手动

| Apifox / Postman 概念 | RustFox 对应 | 方式 |
| --- | --- | --- |
| 接口 + 分组目录 | 接口 + 文件夹 | OpenAPI / Postman Collection 导入，自动建目录 |
| 接口描述、参数、Body、响应示例 | 同名内容 | 导入自动带入 |
| 前置 URL / 环境域名 | 环境 base_url（多模块为各模块基址） | 手动填一行 |
| 环境变量 / 全局变量 | 环境变量 / 项目变量 / 全局变量（`{{name}}` 通用） | 手动对照填；Postman Environment 文件可直接导入 |
| Bearer / Basic / API Key | 同名认证 | 手动照抄（密钥不随文档走） |
| OAuth2 登录态 | OAuth2 四模式 | 需重新走授权拿 token |
| Mock 数据 | Mock 规则 / 响应示例 | 手动重建（规则支持方法 + 路径 + Header + Body 匹配） |
| 后置操作 / 断言 | JSON 测试脚本（`pre_request` / `extract` / `assertions`）+ 测试用例 | 手动重写（§5 有概念对照） |
| 请求历史、Cookie 登录态、自动化 Runner 配置 | — | 不迁移，重新登录/配置 |

## 2. 导出：从源工具拿出 OpenAPI

**Apifox**：项目设置 → 导出 OpenAPI（JSON / YAML 均可；3.0 / 3.1 都行，
3.1 导入时会自动归一化为 3.0 子集，顶层 `webhooks` 会被丢弃）。

**Postman**：Collection → Export v2.1（JSON）。

一次导不全、文件超大时可按模块分多次导出导入（导入均为追加，不覆盖已有数据）。

## 3. 导入：三种入口，结果一样

- 目录工具栏「＋ 新建」下拉 → 文档导入；
- 首页 Dropzone / 项目卡片拖拽文件进入；
- 地址栏直粘 cURL（零散接口补录：自动识别方法 / URL / Header / Body / Basic Auth，
  回填到当前接口）。

支持的格式：OpenAPI 3.0 / 3.1、Swagger 2.0、Postman Collection v2.1（JSON 或 YAML）。
导入后核对两点：分组是否成为文件夹、(base_url + 路径) 拼接是否正确。

## 4. 环境与变量：对照填一行

1. 新建环境，把 Apifox 的前置 URL 填入 base_url（多服务项目按模块分别填基址）；
2. 把 Apifox 环境变量逐个填入 RustFox 环境变量，写法不变（两边都是 `{{name}}`，
   优先级：环境 > 项目 > 全局）；
3. 手头有 Postman Environment 文件可直接导入（环境管理弹窗底部），重名自动加后缀；
4. 切换环境发送一条旧接口，确认域名与变量解析正确。

## 5. 认证、Mock、测试：手动重建对照

- **认证**：Bearer Token / 用户名密码 / API Key 照抄到接口 Auth；
  OAuth2 在设置里重走授权；Cookie 登录态带不过来，重新登录一次即可回放。
- **Mock**：Apifox 的 Mock 期望 → RustFox Mock 规则（方法 + 路径 + Header + Body
  匹配，优先级高于响应示例）；简单固定响应可直接存为「响应示例」兜底。
- **断言/后置操作**：Apifox 的「后置操作 → 提取变量」对应 `extract`，
  「断言」对应 `assertions`（状态码 / 响应体包含 / JSONPath / 耗时），
  多组参数组合存为「测试用例」，再用「全部运行」做集合回归。

## 6. 迁移后冒烟（5 分钟）

1. 切换每个环境，各发送一条核心接口（200 即过）；
2. 跑一遍含断言的测试用例集合；
3. 导出一份冒烟文档归档（测试用例区「导出冒烟文档」）；
4. 备份一次（设置 → 备份 JSON），新环境恢复演练以它为准。

遇到导入报错或字段丢失，提 Issue 时附上（脱敏后的）源文件片段即可复现：
<https://github.com/weihubeats/RustFox/issues>。
