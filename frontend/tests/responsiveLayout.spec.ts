import { expect, test, type Page } from "@playwright/test";
import { installLayoutFixture, layoutAccountName, layoutSupplierName } from "./support/layoutFixture.mjs";

const viewportSizes = [
  { width: 390, height: 844 },
  { width: 768, height: 700 },
  { width: 1024, height: 768 },
  { width: 1280, height: 850 },
  { width: 1440, height: 900 },
  { width: 1920, height: 1080 },
];

// 大于手机宽度时展开导航，以最小可用内容宽度检验布局；手机保留自动折叠行为。
async function resizeWithSidebar(page: Page, viewport: {width: number; height: number}) {
  await page.setViewportSize(viewport);
  if (viewport.width < 768) return;
  const expandSidebar = page.getByRole("button", { name: "展开侧边栏", exact: true });
  if (await expandSidebar.count()) await expandSidebar.click();
}

// 检查真实几何尺寸而不是仅检测 overflow:hidden；卡片、单元格与按钮均应留在内容列中。
async function verifyHorizontalBounds(page: Page) {
  await expect.poll(() => page.evaluate(() => {
    const content = document.querySelector<HTMLElement>('[data-slot="app-content"]');
    if (!content) return ["内容区未挂载"];
    const bounds = content.getBoundingClientRect();
    const overflow = [...content.querySelectorAll<HTMLElement>('[data-slot="card"], [data-slot="table-cell"], [data-slot="table-container"], button')]
      .filter(element => element.getClientRects().length > 0)
      .filter(element => {
        const rectangle = element.getBoundingClientRect();
        return rectangle.left < bounds.left - 1 || rectangle.right > bounds.right + 1;
      })
      .map(element => `${innerWidth}px ${element.dataset.slot ?? element.tagName}: ${element.textContent?.slice(0, 30)}`);
    for (const cell of content.querySelectorAll<HTMLElement>('[data-slot="table-cell"]')) {
      if (cell.getClientRects().length && cell.scrollHeight > cell.clientHeight + 2) overflow.push(`${innerWidth}px ${cell.textContent?.slice(0,30)} 高度 ${cell.clientHeight}/${cell.scrollHeight}`);
    }
    if (content.scrollWidth > content.clientWidth + 1) overflow.push("内容区存在横向溢出");
    return overflow;
  })).toEqual([]);
}

// 同一页连续缩放，验证侧栏、容器查询与账号操作列高度会随窗口变化，而非只在首次渲染时正确。
test("账号池在窗口缩放后保留全部字段和操作", async ({ page }) => {
  await installLayoutFixture(page);
  await page.goto("/accounts/");
  await expect(page.getByText(layoutAccountName, { exact: true }).first()).toBeVisible();
  for (const viewport of viewportSizes) {
    await resizeWithSidebar(page, viewport);
    await verifyHorizontalBounds(page);
    await expect(page.getByRole("button", { name: "编辑账号信息", exact: true })).toBeVisible();
    const labels = page.locator('[data-slot="table-cell-label"]');
    const tableWidth = await page.locator(".account-pool-main-pane").evaluate(element => element.clientWidth);
    if (tableWidth <= 760) {
      await expect(labels.filter({ hasText: "账号代理" })).toBeVisible();
      await expect(labels.filter({ hasText: "状态" })).toBeVisible();
    }
  }
});

// 使用有数据的聚合列表检验长名称、链接、工具按钮以及窄屏字段标签，不以空表通过代替实际布局验证。
test("聚合列表的长文本与操作不会撑出窗口", async ({ page }) => {
  await installLayoutFixture(page);
  await page.goto("/aggregate-api/");
  await expect(page.getByText(layoutSupplierName, { exact: true })).toBeVisible();
  for (const viewport of viewportSizes) {
    await resizeWithSidebar(page, viewport);
    await verifyHorizontalBounds(page);
    await expect(page.getByRole("button", { name: "编辑聚合 API", exact: true })).toBeVisible();
  }
});

// 手机抽屉只覆盖导航侧，不再次压缩正文；收起后恢复同一宽度，避免人为展开造成比例失调。
test("手机展开导航不会挤压正文", async ({ page }) => {
  await installLayoutFixture(page);
  await page.setViewportSize(viewportSizes[0]);
  await page.goto("/aggregate-api/");
  await expect(page.getByText(layoutSupplierName, { exact: true })).toBeVisible();
  const content = page.locator('[data-slot="app-main-column"]');
  const originalWidth = await content.evaluate(element => element.clientWidth);
  await page.getByRole("button", { name: "展开侧边栏", exact: true }).click();
  await expect(content).toHaveJSProperty("clientWidth", originalWidth);
  await verifyHorizontalBounds(page);
  await page.locator('[data-slot="app-sidebar"]').getByRole("button", { name: "收起侧边栏", exact: true }).click();
  await expect(content).toHaveJSProperty("clientWidth", originalWidth);
});

// 对共享壳层和常用表单做跨页面回归；窄窗先检查加载完毕，再检验所有可见控件的真实边界。
for (const routePath of ["/apikeys/", "/models/", "/logs/", "/settings/"]) {
  test(`${routePath} 页面适配窄窗与非全屏`, async ({ page }) => {
    await installLayoutFixture(page);
    await page.goto(routePath);
    await expect(page.getByText("正在准备环境", {exact: true})).not.toBeVisible();
    for (const viewport of viewportSizes.slice(0, 4)) {
      await resizeWithSidebar(page, viewport);
      await verifyHorizontalBounds(page);
    }
  });
}
