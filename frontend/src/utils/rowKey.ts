/**
 * rowKey：动态列表（可编辑 / 可删除）的稳定行 key。
 *
 * `v-for :key="i"` 用索引当 key 时，中间删一行会让后续行的 DOM 与数据错位
 * （输入框焦点、光标、勾选态留在原索引上），这里给行对象本身发一个进程内唯一 id：
 *
 * - 以对象身份（WeakMap）取 id，行对象被就地修改（v-model 写字段）时 id 不变；
 * - 行对象被替换（父级整组回传新对象）时自然拿到新 id，等价于「这一行变了」；
 * - WeakMap 不阻止行对象被回收，删除的行不会泄漏。
 *
 * 注意：行对象若在每次输入时被重建（如 `{...row}` 浅拷贝回传），id 会随之变化，
 * 这类组件要先改成就地更新（或做 id 迁移），否则换稳定 key 反而会丢焦点。
 */
let seq = 0
const ids = new WeakMap<object, string>()

export function rowKey(row: object): string {
  let id = ids.get(row)
  if (id === undefined) {
    seq += 1
    id = `rk${seq}`
    ids.set(row, id)
  }
  return id
}
