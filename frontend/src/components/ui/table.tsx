"use client"

import * as React from "react"

import { cn } from "@/lib/utils"

const tableLabelsContext = React.createContext<string[]>([])

// 从表头提取纯文本供窄布局使用；忽略图标和事件行为，避免复制排序按钮或全选控件。
function collectHeaderText(content: React.ReactNode): string {
  return React.Children.toArray(content).map((child) => {
    if (typeof child === "string" || typeof child === "number") return String(child)
    if (!React.isValidElement<{ children?: React.ReactNode }>(child)) return ""
    return collectHeaderText(child.props.children)
  }).join(" ").trim()
}

// 只读取当前表头的单元格标签；数组与 Fragment 按原顺序展开，空标签不会生成装饰文本。
function collectColumnLabels(content: React.ReactNode): string[] {
  return React.Children.toArray(content).flatMap((child) => {
    if (!React.isValidElement<{ children?: React.ReactNode; colSpan?: number }>(child)) return []
    if (child.type === TableHead) {
      return Array.from({ length: child.props.colSpan ?? 1 }, () => collectHeaderText(child.props.children))
    }
    if (child.type === TableHeader || child.type === TableRow || child.type === React.Fragment) {
      return collectColumnLabels(child.props.children)
    }
    return []
  })
}

// 共享表格按自身容器宽度重排；保留原 table 语义和调用参数，窄布局额外显示纯文本字段名，不复制交互控件。
function Table({ className, children, ...props }: React.ComponentProps<"table">) {
  const labels = collectColumnLabels(children)
  return (
    <div
      data-slot="table-container"
      className="relative w-full min-w-0 max-w-full overflow-x-auto"
    >
      <tableLabelsContext.Provider value={labels}>
      <table
        data-slot="table"
        className={cn("w-full border-separate border-spacing-0 caption-bottom text-sm", className)}
        {...props}
      >{children}</table>
      </tableLabelsContext.Provider>
    </div>
  )
}

/**
 * 函数 `TableHeader`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
function TableHeader({ className, ...props }: React.ComponentProps<"thead">) {
  return (
    <thead
      data-slot="table-header"
      className={cn("[&_tr]:border-b", className)}
      {...props}
    />
  )
}

/**
 * 函数 `TableBody`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
function TableBody({ className, ...props }: React.ComponentProps<"tbody">) {
  return (
    <tbody
      data-slot="table-body"
      className={cn("[&_tr:last-child]:border-0", className)}
      {...props}
    />
  )
}

/**
 * 函数 `TableFooter`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
function TableFooter({ className, ...props }: React.ComponentProps<"tfoot">) {
  return (
    <tfoot
      data-slot="table-footer"
      className={cn(
        "border-t bg-[var(--table-footer-bg)] font-medium [&>tr]:last:border-b-0",
        className
      )}
      {...props}
    />
  )
}

// 为数据行单元格附加对应表头标签；跨列汇总保持原样，行上的选择、悬停及事件参数原样透传。
function TableRow({ className, children, ...props }: React.ComponentProps<"tr">) {
  const labels = React.useContext(tableLabelsContext)
  let columnIndex = 0
  const cells: React.ReactNode[] = []
  for (const child of React.Children.toArray(children)) {
    if (!React.isValidElement<React.ComponentProps<typeof TableCell>>(child)) {
      cells.push(child)
      continue
    }
    const columnSpan = child.props.colSpan ?? 1
    cells.push(child.type === TableCell && columnSpan === 1
      ? React.cloneElement(child, { responsiveLabel: labels[columnIndex] })
      : child)
    columnIndex += columnSpan
  }
  return (
    <tr
      data-slot="table-row"
      className={cn("border-b transition-colors", className)}
      {...props}
    >{cells}</tr>
  )
}

/**
 * 函数 `TableHead`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
function TableHead({ className, ...props }: React.ComponentProps<"th">) {
  return (
    <th
      data-slot="table-head"
      className={cn(
        "h-10 px-2 text-left align-middle font-medium whitespace-nowrap text-foreground [&:has([role=checkbox])]:pr-0",
        className
      )}
      {...props}
    />
  )
}

// 标准单元格只渲染一份业务内容；字段标签由行注入，宽表隐藏标签，窄布局显示标签且不截断内容。
function TableCell({ className, children, responsiveLabel, ...props }: React.ComponentProps<"td"> & { responsiveLabel?: string }) {
  return (
    <td
      data-slot="table-cell"
      className={cn(
        "p-2 align-middle whitespace-nowrap [&:has([role=checkbox])]:pr-0",
        className
      )}
      {...props}
    >
      {responsiveLabel ? <span data-slot="table-cell-label">{responsiveLabel}</span> : null}
      {children}
    </td>
  )
}

/**
 * 函数 `TableCaption`
 *
 * 作者: gaohongshun
 *
 * 时间: 2026-04-02
 *
 * # 参数
 * - params: 参数 params
 *
 * # 返回
 * 返回函数执行结果
 */
function TableCaption({
  className,
  ...props
}: React.ComponentProps<"caption">) {
  return (
    <caption
      data-slot="table-caption"
      className={cn("mt-4 text-sm text-muted-foreground", className)}
      {...props}
    />
  )
}

export {
  Table,
  TableHeader,
  TableBody,
  TableFooter,
  TableHead,
  TableRow,
  TableCell,
  TableCaption,
}
