# Jev / TypeSafe System One

在提供商设置选择 **Jev**，保存后自动创建 **Jev System One** 端点。上游地址为 `https://api.typesafe.ai/v1/systemone`，协议签名为 `typesafe:systemone`。在密钥管理或号池的 **导入 Key** 中直接粘贴 API Key，一行一个，也支持 `名称----Key`；名称可以自动生成。上游请求以 `Authorization: Bearer <上游 API Key>` 认证。

刷新模型调用上游 `GET /v1/models`，读取原生 `models` 数组中的 `name`、`description` 和 `release_date`。模型选择支持 `jev-latest`、`jev-preview`；需要固定版本时可手动填写版本 ID，例如 `jev-1.13.0`，即使该版本不在上游列表中。关联本地全局模型后，客户端使用全局模型名，Aether 只把请求的 `model` 替换为配置的上游模型名。

客户端使用自己的 Aether API Key，并在权限中开放 `typesafe:systemone`、对应模型及 Jev 提供商。原生协议保持独立，聊天协议权限不会自动开放 System One。

## 客户端接口

| 操作 | Aether 路径 | 返回结构 |
| --- | --- | --- |
| 结构化判断 | `POST /v1/systemone` 或 `POST /jev/v1/systemone` | `model`、`answers`、`usage` |
| 原生模型列表 | `GET /jev/v1/models` | `models` 数组，各项含 `name`、`description`、`release_date` |

官方 SDK 的 API 根地址设为 `https://你的网关/jev`，SDK 自动追加 `/v1`。这个前缀让原生模型列表与网关已有的 OpenAI 模型列表共存。模型列表仅返回当前 API Key 可以使用的已配置全局模型；描述和发布日期来自已拉取的上游目录。手动配置且目录未列出的版本保留可调用性，未知发布日期返回空字符串。

```bash
curl https://你的网关/jev/v1/models \
  -H "Authorization: Bearer $AETHER_API_KEY"

curl https://你的网关/jev/v1/systemone \
  -H "Authorization: Bearer $AETHER_API_KEY" \
  -H 'Content-Type: application/json' \
  --data-binary @request.json
```

`request.json` 同时示范全部三种问题：

```json
{
  "model": "jev-latest",
  "state": {
    "message": "I was charged twice. Please refund the duplicate payment.",
    "amount": 25
  },
  "questions": {
    "needs_action": {
      "type": "noul",
      "instructions": "Does this request need action?",
      "criteria": {
        "true": { "description": "A concrete issue needs resolution" },
        "false": "No action is needed"
      }
    },
    "department": {
      "type": "choice",
      "instructions": { "task": "Select the responsible department" },
      "criteria": {
        "billing": "Payment or refund issue",
        "technical": ["An error in the product"],
        "other": null
      }
    },
    "urgency": {
      "type": "score",
      "instructions": ["Evaluate urgency"],
      "criteria": ["Low", "Medium", "High"]
    }
  }
}
```

`state` 和每个问题的 `instructions` 都接受字符串、JSON 对象或数组。Noul 的 `criteria` 可省略，提供时可描述 `true`、`false`。Choice 使用选项名到说明的映射，说明也可以是对象、数组或 `null`，最多 255 个选项。Score 使用有序等级数组，必须有 2 到 10 个等级。请求中的结构化数据原样传递。

响应中的 Noul 返回 0 到 1 的 `noul` 概率；Choice 返回 `choice`、全部选项的 `probabilities` 和 `confidence`；Score 返回允许小数的 `score`、`legend`、全部等级的 `probabilities` 和 `confidence`。Aether 保留全部原生字段，响应 `model` 保留实际回答的版本 ID，不改回客户端使用的别名。

Python 官方 SDK 示例：

```python
import os
from typesafe_sdk import TypeSafeClient, Noul

with TypeSafeClient(
    api_key=os.environ["AETHER_API_KEY"],
    base_url="https://你的网关/jev",
    model="jev-latest",
) as client:
    result = client.system_one(
        {"message": "I was charged twice."},
        {"billing": Noul(instructions="Is this about billing?")},
    )
    print(result.model, result.nouls["billing"].noul, result.usage)
    print(client.models.list().models)
```

JavaScript SDK 同样将 `apiKey` 设为 Aether Key、`baseURL` 设为 `https://你的网关/jev`，并使用已经配置的模型名。

## 用量、限制与错误

原生 `usage.input_tokens` 和 `usage.output_tokens` 都进入每条使用记录，缺少 `total_tokens` 时按输入和输出相加。官方当前只收取输入费用，输出免费；配置模型价格时，上游成本参考输入 **$0.042 / 百万 token**、输出 **0**，对用户的售价按管理员配置执行。

Jev 接受文本及文本的结构化描述。官方当前上下文限制为总计 64k token，`state` 加最长一个问题不超过 32k；token 预算由上游验证。该接口使用同步 JSON，不支持流式输出、多模态输入或聊天请求字段。模型测试默认生成有效的 Noul 请求，也可以在请求体编辑器中测试 Choice、Score 或混合问题。

认证失败返回 401，格式或上下文验证失败返回 422。上游的 422 验证详情保持原生结构；429 限流和 529 过载进入现有号池冷却、候选切换机制，最终返回时保留状态及 `retry-after`。客户端继续使用官方 SDK 的指数退避策略，HTTP 调用者应遵循 `retry-after`，不要立即反复重试。上游速率配额动态调整，不在网关中写死。

官方参考：[API](https://docs.typesafe.ai/api)、[模型与计费](https://docs.typesafe.ai/models)、[Python SDK](https://docs.typesafe.ai/sdk/python)。
