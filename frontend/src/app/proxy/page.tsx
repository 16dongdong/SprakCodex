"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { invokeProxy as invoke } from "./invokeProxy";
import type { ConfigView, ProxyConfig, ProxyGroup, ProxyNode, Subscription } from "./types";
import { IosSwitch } from "./Switch";
import { Hint } from "./Hint";
import "flag-icons/css/flag-icons.min.css";

function normalizeNode(node: ProxyNode): ProxyNode {
  return { ...node, chain_entry: node.chain_entry ?? null, sub: node.sub ?? null };
}

function normalizeProxy(proxy: ProxyConfig): ProxyConfig {
  return {
    ...proxy,
    mitm: proxy.mitm ?? false,
    nodes: (proxy.nodes ?? []).map(normalizeNode),
    groups: (proxy.groups ?? []).map((g) => ({ ...g, members: g.members ?? [], selected: g.selected ?? null })),
    subscriptions: proxy.subscriptions ?? [],
  };
}

interface EgressView {
  via: string;
  ip: string;
  country: string;
  city: string;
  isp: string;
  error?: string | null;
}

function delayClass(ms: number | null | undefined): string {
  if (ms == null) return "d-bad";
  if (ms < 200) return "d-good";
  if (ms < 500) return "d-ok";
  if (ms < 900) return "d-mid";
  return "d-bad";
}

function hostOf(url: string): string {
  return url.replace(/^https?:\/\//, "").split("/")[0] || url;
}

function formatRelTime(sec: number): string {
  if (!sec) return "从未更新";
  const diff = Math.floor(Date.now() / 1000) - sec;
  if (diff < 60) return "刚刚更新";
  if (diff < 3600) return `${Math.floor(diff / 60)} 分钟前`;
  if (diff < 86400) return `${Math.floor(diff / 3600)} 小时前`;
  return `${Math.floor(diff / 86400)} 天前`;
}

const KIND_LABEL: Record<string, string> = {
  trojan: "Trojan",
  socks5: "SOCKS5",
  http: "HTTP",
  https: "HTTPS",
};

// WebView2 的 dataTransfer 在跨嵌套按钮拖动时可能丢失文本，进程内变量保留同一次拖动的节点身份。
let draggedNodeName = "";
let pointerDragId: number | null = null;
let pointerStart = { x: 0, y: 0 };
let pointerDragMoved = false;
let suppressNodeClick = false;

function clearPointerDropHighlight() {
  document.querySelectorAll("[data-proxy-group].drag-over").forEach((element) => {
    element.classList.remove("drag-over");
  });
}

// 按节点名关键词推断地区国家码(小写 ISO);仅作兜底,测速后用真实出口国家码覆盖。
const CC_RULES: [RegExp, string][] = [
  [/香港|hong\s?kong|\bHK\b/i, "hk"],
  [/台湾|台灣|taiwan|\bTW\b/i, "tw"],
  [/日本|japan|\bJP\b|东京|大阪/i, "jp"],
  [/新加坡|singapore|狮城|\bSG\b/i, "sg"],
  [/韩国|韓國|korea|首尔|\bKR\b/i, "kr"],
  [/美国|united\s?states|\bUSA?\b|洛杉矶|圣何塞|纽约|硅谷|西雅图|达拉斯/i, "us"],
  [/英国|united\s?kingdom|\bUK\b|\bGB\b|伦敦/i, "gb"],
  [/德国|germany|\bDE\b|法兰克福/i, "de"],
  [/法国|france|\bFR\b|巴黎/i, "fr"],
  [/加拿大|canada|\bCA\b/i, "ca"],
  [/俄罗斯|russia|\bRU\b|莫斯科/i, "ru"],
  [/印度|india|\bIN\b|孟买/i, "in"],
  [/澳大利亚|澳洲|australia|\bAU\b|悉尼/i, "au"],
  [/荷兰|netherlands|\bNL\b|阿姆斯特丹/i, "nl"],
  [/土耳其|turkey|\bTR\b/i, "tr"],
  [/巴西|brazil|\bBR\b/i, "br"],
  [/马来|malaysia|\bMY\b/i, "my"],
  [/泰国|thailand|\bTH\b/i, "th"],
  [/越南|vietnam|\bVN\b/i, "vn"],
  [/菲律宾|philippines|\bPH\b/i, "ph"],
];

// 名字里自带的国旗 emoji(regional indicator 对)→ 小写 ISO 国家码(🇺🇸 → us)。
function flagEmojiToCC(s: string): string {
  const cps = [...s].map((c) => c.codePointAt(0) || 0);
  const a = 0x1f1e6;
  if (cps.length === 2 && cps[0] >= a && cps[0] <= 0x1f1ff && cps[1] >= a && cps[1] <= 0x1f1ff) {
    return String.fromCharCode(cps[0] - a + 97) + String.fromCharCode(cps[1] - a + 97);
  }
  return "";
}

/** 取节点地区国家码(小写 ISO)与去旗后的显示名:名字自带国旗 emoji 优先,否则按关键词推断。 */
function nodeCC(name: string): { cc: string; label: string } {
  const m = name.match(/(\p{Regional_Indicator}\p{Regional_Indicator})/u);
  if (m) {
    return { cc: flagEmojiToCC(m[1]), label: name.replace(m[1], "").trim() || name };
  }
  for (const [re, cc] of CC_RULES) {
    if (re.test(name)) return { cc, label: name };
  }
  return { cc: "", label: name };
}

export default function ProxyPage() {
  const [proxy, setProxy] = useState<ProxyConfig>({
    enabled: false,
    active: null,
    nodes: [],
    groups: [],
    mitm: false,
    subscriptions: [],
  });
  const [tab, setTab] = useState<"nodes" | "subs">("nodes");
  const [error, setError] = useState("");
  const [savedFlash, setSavedFlash] = useState(false);
  const [delays, setDelays] = useState<Record<string, number | null>>({});
  const [countries, setCountries] = useState<Record<string, string>>({});
  const [testing, setTesting] = useState(false);
  const [testingNode, setTestingNode] = useState<string | null>(null);
  const [egress, setEgress] = useState<EgressView | null>(null);
  const [egTesting, setEgTesting] = useState(false);
  const [addNodeOpen, setAddNodeOpen] = useState(false);
  const [addSubOpen, setAddSubOpen] = useState(false);
  const [addGroupOpen, setAddGroupOpen] = useState(false);
  const [chainPicking, setChainPicking] = useState(false);
  const [subBusy, setSubBusy] = useState<string | null>(null);
  const savedFlashTimer = useRef<number | null>(null);
  // 配置加载完成前不渲染交互界面、不保存:避免「加载途中用户已改动 → 迟到的 get_config 覆盖
  // 并存回旧数据」的竞态(典型表现:刚加的订阅下次打开丢失、节点退回旧的本地状态)。
  const [ready, setReady] = useState(false);

  useEffect(() => {
    invoke<ConfigView>("get_config")
      .then((config) => {
        setProxy(normalizeProxy(config.proxy));
        setReady(true);
      })
      .catch((reason) => {
        setError(String(reason));
        setReady(true);
      });
  }, []);

  useEffect(() => {
    if (!ready) return;
    const id = window.setTimeout(() => {
      invoke("save_proxy_config", {
        proxy: {
          enabled: proxy.enabled,
          active: proxy.active,
          nodes: proxy.nodes.map(normalizeNode),
          groups: proxy.groups,
          mitm: proxy.mitm,
          subscriptions: proxy.subscriptions,
        },
      })
        .then(() => {
          setSavedFlash(true);
          if (savedFlashTimer.current !== null) window.clearTimeout(savedFlashTimer.current);
          savedFlashTimer.current = window.setTimeout(() => {
            savedFlashTimer.current = null;
            setSavedFlash(false);
          }, 1400);
        })
        .catch((reason) => setError(String(reason)));
    }, 450);
    return () => window.clearTimeout(id);
  }, [proxy, ready]);

  useEffect(
    () => () => {
      if (savedFlashTimer.current !== null) window.clearTimeout(savedFlashTimer.current);
    },
    [],
  );

  // 后台静默测速:节点持续显示当前延迟(首测留点内核启动时间,之后每 30s 刷新一次)。
  const nodeCount = proxy.nodes.length;
  useEffect(() => {
    if (!ready || nodeCount === 0) return;
    let alive = true;
    const tick = async () => {
      try {
        const results = await invoke<
          { name: string; delay: number | null; country_code?: string | null }[]
        >("test_nodes");
        if (!alive) return;
        const nd: Record<string, number | null> = {};
        const nc: Record<string, string> = {};
        for (const r of results) {
          nd[r.name] = r.delay;
          if (r.country_code) nc[r.name] = r.country_code.toLowerCase();
        }
        setDelays((cur) => ({ ...cur, ...nd }));
        setCountries((cur) => ({ ...cur, ...nc }));
      } catch {
        /* 静默:后台测速失败不打扰 */
      }
    };
    const first = window.setTimeout(tick, 3000);
    const iv = window.setInterval(tick, 30000);
    return () => {
      alive = false;
      window.clearTimeout(first);
      window.clearInterval(iv);
    };
  }, [nodeCount, ready]);

  const patchProxy = (patch: Partial<ProxyConfig>) =>
    setProxy((current) => ({ ...current, ...patch }));

  const setActive = (name: string) => patchProxy({ active: name });

  const addNodes = (newNodes: ProxyNode[]) => {
    setProxy((current) => {
      const byName = new Map(current.nodes.map((node) => [node.name, node]));
      for (const node of newNodes) byName.set(node.name, node);
      return {
        ...current,
        nodes: Array.from(byName.values()),
        active: current.active ?? newNodes[0]?.name ?? null,
        enabled: current.nodes.length === 0 ? true : current.enabled,
      };
    });
    setAddNodeOpen(false);
  };

  const removeNode = (name: string) =>
    setProxy((current) => ({
      ...current,
      nodes: current.nodes
        .filter((node) => node.name !== name)
        .map((node) => (node.chain_entry === name ? { ...node, chain_entry: null } : node)),
      active: current.active === name ? null : current.active,
    }));

  const setChainEntry = (exitName: string, entryName: string | null) =>
    setProxy((current) => ({
      ...current,
      nodes: current.nodes.map((node) =>
        node.name === exitName
          ? { ...node, chain_entry: entryName === exitName ? null : entryName }
          : node,
      ),
    }));

  // ---- 代理组 ----
  const addGroup = (name: string, kind: "select" | "url-test") => {
    if (proxy.groups.some((g) => g.name === name) || proxy.nodes.some((n) => n.name === name)) {
      setError(`组名「${name}」与已有组或节点重名`);
      return;
    }
    setProxy((c) => ({ ...c, groups: [...c.groups, { name, kind, members: [], selected: null }] }));
    setAddGroupOpen(false);
  };

  const removeGroup = (name: string) =>
    setProxy((c) => ({
      ...c,
      groups: c.groups.filter((g) => g.name !== name),
      // 镜像节点删除的清理:以本组为链式入口的节点清空 chain_entry;本组是当前出口则置空。
      nodes: c.nodes.map((n) => (n.chain_entry === name ? { ...n, chain_entry: null } : n)),
      active: c.active === name ? null : c.active,
    }));

  const addMember = (groupName: string, nodeName: string) => {
    const node = proxy.nodes.find((n) => n.name === nodeName);
    if (!node) return;
    if (node.chain_entry === groupName) {
      setError(`「${nodeName}」以「${groupName}」为链式入口,不能再放进该组(会成环)`);
      return;
    }
    setProxy((c) => ({
      ...c,
      groups: c.groups.map((g) =>
        g.name === groupName && !g.members.includes(nodeName)
          ? { ...g, members: [...g.members, nodeName] }
          : g,
      ),
    }));
  };

  const removeMember = (groupName: string, nodeName: string) =>
    setProxy((c) => ({
      ...c,
      groups: c.groups.map((g) =>
        g.name === groupName
          ? {
              ...g,
              members: g.members.filter((m) => m !== nodeName),
              selected: g.selected === nodeName ? null : g.selected,
            }
          : g,
      ),
    }));

  const selectGroupMember = (groupName: string, member: string) => {
    setProxy((c) => ({
      ...c,
      groups: c.groups.map((g) => (g.name === groupName ? { ...g, selected: member } : g)),
    }));
    // 即时切到内核(不等保存);selected 的持久化由自动保存完成。
    invoke("select_group_member", { group: groupName, member }).catch((r) => setError(String(r)));
  };

  // ---- 订阅 ----
  const applySubscription = (sub: Subscription, fetched: ProxyNode[]) => {
    const tagged = fetched.map((n) => ({ ...n, sub: sub.name }));
    setProxy((current) => {
      const others = current.nodes.filter((n) => n.sub !== sub.name);
      const merged = [...others, ...tagged];
      const subs = current.subscriptions.some((s) => s.name === sub.name)
        ? current.subscriptions.map((s) => (s.name === sub.name ? sub : s))
        : [...current.subscriptions, sub];
      return {
        ...current,
        nodes: merged,
        subscriptions: subs,
        active: current.active ?? tagged[0]?.name ?? null,
        enabled: current.nodes.length === 0 && tagged.length > 0 ? true : current.enabled,
      };
    });
  };

  const fetchAndApply = async (name: string, url: string) => {
    setSubBusy(name);
    setError("");
    try {
      const nodes = await invoke<ProxyNode[]>("fetch_subscription", { url });
      if (nodes.length === 0) {
        setError(`订阅「${name}」没解析到节点(请确认是 Clash 订阅链接)`);
        return false;
      }
      applySubscription(
        { name, url, updated_at: Math.floor(Date.now() / 1000), node_count: nodes.length },
        nodes,
      );
      return true;
    } catch (reason) {
      setError(`订阅「${name}」拉取失败:${String(reason)}`);
      return false;
    } finally {
      setSubBusy(null);
    }
  };

  const addSubscription = async (name: string, url: string) => {
    if (proxy.subscriptions.some((s) => s.name === name)) {
      setError(`订阅名「${name}」已存在`);
      return;
    }
    const ok = await fetchAndApply(name, url);
    if (ok) setAddSubOpen(false);
  };

  const removeSubscription = (name: string) =>
    setProxy((current) => {
      const removedNodeNames = new Set(
        current.nodes.filter((n) => n.sub === name).map((n) => n.name),
      );
      return {
        ...current,
        subscriptions: current.subscriptions.filter((s) => s.name !== name),
        nodes: current.nodes
          .filter((n) => n.sub !== name)
          .map((n) =>
            n.chain_entry && removedNodeNames.has(n.chain_entry)
              ? { ...n, chain_entry: null }
              : n,
          ),
        active: current.active && removedNodeNames.has(current.active) ? null : current.active,
      };
    });

  const testNodes = async () => {
    setTesting(true);
    try {
      const results = await invoke<
        { name: string; delay: number | null; error?: string | null; country_code?: string | null }[]
      >("test_nodes");
      const nextDelays: Record<string, number | null> = {};
      const nextCC: Record<string, string> = {};
      for (const result of results) {
        nextDelays[result.name] = result.delay;
        if (result.country_code) nextCC[result.name] = result.country_code.toLowerCase();
      }
      setDelays(nextDelays);
      setCountries((cur) => ({ ...cur, ...nextCC }));
    } catch (reason) {
      setError(String(reason));
    }
    setTesting(false);
  };

  const testNode = async (name: string) => {
    setTestingNode(name);
    try {
      const result = await invoke<{ name: string; delay: number | null }>("test_node", { name });
      setDelays((current) => ({ ...current, [name]: result.delay }));
    } catch (reason) {
      setError(String(reason));
    }
    setTestingNode(null);
  };

  const testEgress = async () => {
    setEgTesting(true);
    setEgress(null);
    try {
      setEgress(await invoke<EgressView>("test_egress"));
    } catch (reason) {
      setError(String(reason));
    }
    setEgTesting(false);
  };

  const activeNode = proxy.nodes.find((n) => n.name === proxy.active) ?? null;
  const activeGroup = proxy.groups.find((g) => g.name === proxy.active) ?? null;
  const manualNodes = proxy.nodes.filter((n) => !n.sub);

  // 配置未加载完成前只显示占位,禁止交互/保存,杜绝加载竞态覆盖。
  if (!ready) {
    return (
      <div className="settings proxy-page cproxy-proxy">
        <div className="empty-card">正在加载代理配置…</div>
      </div>
    );
  }

  return (
    <div className="settings proxy-page cproxy-proxy">
      <div className="proxy-top">
        <div className="settings-h-row">
          <div className="settings-h">代理</div>
          <Hint text="订阅与节点分开管理,改动自动保存,运行中的劫持会按新上游重建。" />
          {savedFlash && <span className="saved-tip">已保存 ✓</span>}
        </div>
        <label className="proxy-enable">
          <span>{proxy.enabled ? "代理已启用" : "原始直连"}</span>
          <IosSwitch on={proxy.enabled} onChange={(enabled) => patchProxy({ enabled })} />
        </label>
      </div>

      {error && (
        <div className="error inline" onClick={() => setError("")}>
          {error}
        </div>
      )}

      {egress && <EgressLine egress={egress} onClose={() => setEgress(null)} />}

      <div className="proxy-tabs">
        <button type="button" className={tab === "nodes" ? "on" : ""} onClick={() => setTab("nodes")}>
          节点 <span className="tab-count">{proxy.nodes.length}</span>
        </button>
        <button type="button" className={tab === "subs" ? "on" : ""} onClick={() => setTab("subs")}>
          订阅 <span className="tab-count">{proxy.subscriptions.length}</span>
        </button>
      </div>

      {tab === "subs" ? (
        <SubsTab
          subscriptions={proxy.subscriptions}
          nodes={proxy.nodes}
          busy={subBusy}
          onAdd={() => setAddSubOpen(true)}
          onUpdate={(s) => fetchAndApply(s.name, s.url)}
          onRemove={removeSubscription}
        />
      ) : (
        <NodesTab
          proxy={proxy}
          delays={delays}
          countries={countries}
          testing={testing}
          testingNode={testingNode}
          egTesting={egTesting}
          activeNode={activeNode}
          activeGroup={activeGroup}
          manualNodes={manualNodes}
          chainPicking={chainPicking}
          onSetActive={setActive}
          onRemoveNode={removeNode}
          onSetChainEntry={setChainEntry}
          onStartChainPick={() => setChainPicking(true)}
          onCancelChainPick={() => setChainPicking(false)}
          onTestNodes={testNodes}
          onTestNode={testNode}
          onTestEgress={testEgress}
          onAddNode={() => setAddNodeOpen(true)}
          onAddGroup={() => setAddGroupOpen(true)}
          onRemoveGroup={removeGroup}
          onAddMember={addMember}
          onRemoveMember={removeMember}
          onSelectMember={selectGroupMember}
        />
      )}

      {addNodeOpen && <AddNodeModal onClose={() => setAddNodeOpen(false)} onAdd={addNodes} />}
      {addGroupOpen && (
        <AddGroupModal
          taken={[...proxy.groups.map((g) => g.name), ...proxy.nodes.map((n) => n.name)]}
          onClose={() => setAddGroupOpen(false)}
          onAdd={addGroup}
        />
      )}
      {addSubOpen && (
        <AddSubModal
          busy={subBusy != null}
          onClose={() => setAddSubOpen(false)}
          onAdd={addSubscription}
        />
      )}
    </div>
  );
}

// ============================ 订阅标签页 ============================

function SubsTab({
  subscriptions,
  nodes,
  busy,
  onAdd,
  onUpdate,
  onRemove,
}: {
  subscriptions: Subscription[];
  nodes: ProxyNode[];
  busy: string | null;
  onAdd: () => void;
  onUpdate: (s: Subscription) => void;
  onRemove: (name: string) => void;
}) {
  return (
    <section className="proxy-pane">
      <div className="pane-toolbar">
        <Hint text="从机场/Clash 订阅链接拉取节点,可随时更新。" />
        <button type="button" className="icon-btn primary" onClick={onAdd}>
          ＋ 添加订阅
        </button>
      </div>
      {subscriptions.length === 0 ? (
        <div className="empty-card">还没有订阅 —— 点「＋ 添加订阅」粘贴订阅链接</div>
      ) : (
        <div className="sub-list">
          {subscriptions.map((s) => {
            const count = nodes.filter((n) => n.sub === s.name).length;
            const loading = busy === s.name;
            return (
              <div key={s.name} className="sub-card">
                <div className="sub-card-main">
                  <div className="sub-card-name">{s.name}</div>
                  <div className="sub-card-url">{hostOf(s.url)}</div>
                </div>
                <div className="sub-card-meta">
                  <span className="sub-badge">{count} 节点</span>
                  <span className="sub-time">{formatRelTime(s.updated_at)}</span>
                </div>
                <div className="sub-card-actions">
                  <button
                    type="button"
                    className="sub-act"
                    title="更新订阅"
                    disabled={loading}
                    onClick={() => onUpdate(s)}
                  >
                    {loading ? "⏳" : "↻"}
                  </button>
                  <button
                    type="button"
                    className="sub-act danger"
                    title="删除订阅及其节点"
                    onClick={() => onRemove(s.name)}
                  >
                    ×
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

// ============================ 节点标签页 ============================

function NodesTab({
  proxy,
  delays,
  countries,
  testing,
  testingNode,
  egTesting,
  activeNode,
  activeGroup,
  manualNodes,
  chainPicking,
  onSetActive,
  onRemoveNode,
  onSetChainEntry,
  onStartChainPick,
  onCancelChainPick,
  onTestNodes,
  onTestNode,
  onTestEgress,
  onAddNode,
  onAddGroup,
  onRemoveGroup,
  onAddMember,
  onRemoveMember,
  onSelectMember,
}: {
  proxy: ProxyConfig;
  delays: Record<string, number | null>;
  countries: Record<string, string>;
  testing: boolean;
  testingNode: string | null;
  egTesting: boolean;
  activeNode: ProxyNode | null;
  activeGroup: ProxyGroup | null;
  manualNodes: ProxyNode[];
  chainPicking: boolean;
  onSetActive: (name: string) => void;
  onRemoveNode: (name: string) => void;
  onSetChainEntry: (exit: string, entry: string | null) => void;
  onStartChainPick: () => void;
  onCancelChainPick: () => void;
  onTestNodes: () => void;
  onTestNode: (name: string) => void;
  onTestEgress: () => void;
  onAddNode: () => void;
  onAddGroup: () => void;
  onRemoveGroup: (name: string) => void;
  onAddMember: (group: string, node: string) => void;
  onRemoveMember: (group: string, node: string) => void;
  onSelectMember: (group: string, member: string) => void;
}) {
  if (proxy.nodes.length === 0) {
    return (
      <section className="proxy-pane">
        <div className="pane-toolbar">
          <Hint text="手动添加,或在「订阅」页导入。" />
          <button type="button" className="icon-btn primary" onClick={onAddNode}>
            ＋ 手动节点
          </button>
        </div>
        <div className="empty-card">还没有节点 —— 添加订阅或手动填一个</div>
      </section>
    );
  }

  return (
    <section className="proxy-pane">
      <div className="pane-toolbar">
        <div className="pane-toolbar-btns">
          <button type="button" className="icon-btn" disabled={testing} onClick={onTestNodes}>
            {testing ? "测速中…" : "⚡ 测速"}
          </button>
          <button type="button" className="icon-btn" disabled={egTesting} onClick={onTestEgress}>
            {egTesting ? "测试中…" : "🌐 测出口"}
          </button>
        </div>
        <button type="button" className="icon-btn primary" onClick={onAddNode}>
          ＋ 手动节点
        </button>
      </div>

      {activeGroup ? (
        <div className="chain-bar">
          <div className="chain-flow">
            <span className="chain-pill exit">{activeGroup.name}</span>
            <span className="chain-tag">
              {activeGroup.kind === "url-test" ? "组出口 · 自动选最优" : "组出口 · 手动选成员"}
            </span>
          </div>
        </div>
      ) : (
        <ChainBar
          activeNode={activeNode}
          nodes={proxy.nodes}
          groups={proxy.groups}
          picking={chainPicking}
          onSetChainEntry={onSetChainEntry}
          onStartPick={onStartChainPick}
          onCancelPick={onCancelChainPick}
        />
      )}

      <GroupsSection
        groups={proxy.groups}
        active={proxy.active}
        delays={delays}
        testingNode={testingNode}
        onAddGroup={onAddGroup}
        onRemoveGroup={onRemoveGroup}
        onAddMember={onAddMember}
        onRemoveMember={onRemoveMember}
        onSelectMember={onSelectMember}
        onSetActive={onSetActive}
        onTestNode={onTestNode}
      />

      {proxy.subscriptions.map((sub) => {
        const group = proxy.nodes.filter((n) => n.sub === sub.name);
        if (group.length === 0) return null;
        return (
          <NodeGroup
            key={sub.name}
            title={sub.name}
            nodes={group}
            active={proxy.active}
            delays={delays}
            countries={countries}
            testingNode={testingNode}
            onSetActive={onSetActive}
            onRemoveNode={onRemoveNode}
            onTestNode={onTestNode}
            onDropNode={onAddMember}
          />
        );
      })}
      {manualNodes.length > 0 && (
        <NodeGroup
          title="本地节点"
          nodes={manualNodes}
          active={proxy.active}
          delays={delays}
          countries={countries}
          testingNode={testingNode}
          onSetActive={onSetActive}
          onRemoveNode={onRemoveNode}
          onTestNode={onTestNode}
          onDropNode={onAddMember}
        />
      )}
    </section>
  );
}

// ============================ 代理组 ============================

function GroupsSection({
  groups,
  active,
  delays,
  testingNode,
  onAddGroup,
  onRemoveGroup,
  onAddMember,
  onRemoveMember,
  onSelectMember,
  onSetActive,
  onTestNode,
}: {
  groups: ProxyGroup[];
  active: string | null;
  delays: Record<string, number | null>;
  testingNode: string | null;
  onAddGroup: () => void;
  onRemoveGroup: (name: string) => void;
  onAddMember: (group: string, node: string) => void;
  onRemoveMember: (group: string, node: string) => void;
  onSelectMember: (group: string, member: string) => void;
  onSetActive: (name: string) => void;
  onTestNode: (name: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="group-section">
      <div className="group-section-head">
        <button type="button" className="group-section-title" onClick={() => setOpen((o) => !o)}>
          <span className={`collapse-caret${open ? " open" : ""}`}>▸</span>
          代理组 <span className="tab-count">{groups.length}</span>
        </button>
        <button type="button" className="icon-btn" onClick={onAddGroup}>
          ＋ 新建组
        </button>
      </div>
      {!open ? null : groups.length === 0 ? (
        <div className="group-empty-hint">
          建个组,把下面的节点拖进来 —— 出口可直接选「组」(手动选成员 / 按延迟自动选最优)。
        </div>
      ) : (
        <div className="group-grid">
          {groups.map((g) => (
            <GroupCard
              key={g.name}
              group={g}
              active={active}
              delays={delays}
              testingNode={testingNode}
              onRemove={onRemoveGroup}
              onAddMember={onAddMember}
              onRemoveMember={onRemoveMember}
              onSelectMember={onSelectMember}
              onSetActive={onSetActive}
              onTestNode={onTestNode}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function GroupCard({
  group,
  active,
  delays,
  testingNode,
  onRemove,
  onAddMember,
  onRemoveMember,
  onSelectMember,
  onSetActive,
  onTestNode,
}: {
  group: ProxyGroup;
  active: string | null;
  delays: Record<string, number | null>;
  testingNode: string | null;
  onRemove: (name: string) => void;
  onAddMember: (group: string, node: string) => void;
  onRemoveMember: (group: string, node: string) => void;
  onSelectMember: (group: string, member: string) => void;
  onSetActive: (name: string) => void;
  onTestNode: (name: string) => void;
}) {
  const [over, setOver] = useState(false);
  const [open, setOpen] = useState(true);
  const on = active === group.name;
  const auto = group.kind === "url-test";
  return (
    <div
      data-proxy-group={group.name}
      className={`group-card${on ? " on" : ""}${over ? " drag-over" : ""}`}
      onDragOver={(e) => {
        e.preventDefault();
        e.dataTransfer.dropEffect = "copy";
        if (!over) setOver(true);
      }}
      onDragLeave={() => setOver(false)}
      onDrop={(e) => {
        e.preventDefault();
        setOver(false);
        const name =
          e.dataTransfer.getData("application/x-sprak-proxy-node") ||
          e.dataTransfer.getData("text/plain") ||
          draggedNodeName;
        if (name) {
          onAddMember(group.name, name);
          setOpen(true); // 拖进来自动展开,好让你看到刚加的节点
        }
        draggedNodeName = "";
      }}
    >
      {/* 头部:点整行展开/收缩(Clash Verge 式);左侧圆点单独设为出口 */}
      <div
        className="group-card-head"
        onClick={() => setOpen((o) => !o)}
        title={open ? "收起" : "展开"}
      >
        <button
          type="button"
          className={`exit-radio${on ? " on" : ""}`}
          title={on ? "当前出口" : "设为当前出口"}
          onClick={(e) => {
            e.stopPropagation();
            onSetActive(group.name);
          }}
        >
          <span className="exit-dot" />
        </button>
        <span className="group-card-name">{group.name}</span>
        <span className={`group-kind ${auto ? "auto" : "manual"}`}>{auto ? "自动·延迟" : "手动"}</span>
        <span className="group-count">{group.members.length} 节点</span>
        <button
          type="button"
          className="node-del"
          title="删除组"
          onClick={(e) => {
            e.stopPropagation();
            onRemove(group.name);
          }}
        >
          ×
        </button>
        <span className={`group-chevron${open ? " open" : ""}`} aria-hidden>
          ›
        </span>
      </div>
      {open && (
        <div className="group-tiles">
          {group.members.length === 0 ? (
            <div className="group-drop-zone">把下方节点拖到这里加入本组</div>
          ) : (
            group.members.map((m) => {
              const sel = !auto && group.selected === m;
              const ms = delays[m];
              return (
                <div
                  key={m}
                  className={`member-tile${sel ? " sel" : ""}${auto ? " auto" : ""}`}
                  title={auto ? "url-test 自动按延迟选路" : "点选为该组当前出口"}
                  onClick={() => {
                    if (!auto) onSelectMember(group.name, m);
                  }}
                >
                  <button
                    type="button"
                    className="member-del"
                    title="移出组"
                    onClick={(e) => {
                      e.stopPropagation();
                      onRemoveMember(group.name, m);
                    }}
                  >
                    ×
                  </button>
                  <div className="member-tile-name">
                    {sel && <span className="node-tick">✓</span>}
                    {nodeCC(m).label}
                  </div>
                  <div className="member-tile-foot">
                    <span className="member-tile-kind">{auto ? "自动" : "成员"}</span>
                    <button
                      type="button"
                      className={`delay delay-action ${ms == null ? "" : delayClass(ms)}`}
                      disabled={testingNode === m}
                      onClick={(event) => {
                        event.stopPropagation();
                        onTestNode(m);
                      }}
                    >
                      {testingNode === m ? "…" : ms == null ? "测速" : `${ms} ms`}
                    </button>
                  </div>
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}

function NodeGroup({
  title,
  nodes,
  active,
  delays,
  countries,
  testingNode,
  onSetActive,
  onRemoveNode,
  onTestNode,
  onDropNode,
}: {
  title: string;
  nodes: ProxyNode[];
  active: string | null;
  delays: Record<string, number | null>;
  countries: Record<string, string>;
  testingNode: string | null;
  onSetActive: (name: string) => void;
  onRemoveNode: (name: string) => void;
  onTestNode: (name: string) => void;
  onDropNode: (group: string, node: string) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <div className="node-group">
      <button type="button" className="node-group-title" onClick={() => setOpen((o) => !o)}>
        <span className={`collapse-caret${open ? " open" : ""}`}>▸</span>
        {title} <span className="tab-count">{nodes.length}</span>
      </button>
      {open && (
      <div className="node-grid">
        {nodes.map((node) => {
          const on = active === node.name;
          const ms = delays[node.name];
          const meta = nodeCC(node.name);
          const cc = countries[node.name] || meta.cc;
          return (
            <div
              key={node.name}
              className={`node-card${on ? " on" : ""}`}
              onPointerDown={(event) => {
                if (event.button !== 0 || (event.target as HTMLElement).closest("button")) return;
                pointerDragId = event.pointerId;
                pointerStart = { x: event.clientX, y: event.clientY };
                pointerDragMoved = false;
                draggedNodeName = node.name;
                event.currentTarget.setPointerCapture(event.pointerId);
              }}
              onPointerMove={(event) => {
                if (pointerDragId !== event.pointerId || draggedNodeName !== node.name) return;
                if (Math.hypot(event.clientX - pointerStart.x, event.clientY - pointerStart.y) < 6) return;
                pointerDragMoved = true;
                clearPointerDropHighlight();
                document
                  .elementFromPoint(event.clientX, event.clientY)
                  ?.closest("[data-proxy-group]")
                  ?.classList.add("drag-over");
              }}
              onPointerUp={(event) => {
                if (pointerDragId !== event.pointerId || draggedNodeName !== node.name) return;
                const group = document
                  .elementFromPoint(event.clientX, event.clientY)
                  ?.closest<HTMLElement>("[data-proxy-group]")
                  ?.dataset.proxyGroup;
                if (pointerDragMoved && group) onDropNode(group, node.name);
                suppressNodeClick = pointerDragMoved;
                pointerDragId = null;
                draggedNodeName = "";
                clearPointerDropHighlight();
                window.setTimeout(() => {
                  suppressNodeClick = false;
                }, 0);
              }}
              onPointerCancel={() => {
                pointerDragId = null;
                draggedNodeName = "";
                clearPointerDropHighlight();
              }}
              onClick={() => {
                if (!suppressNodeClick) onSetActive(node.name);
              }}
              title="拖到上方代理组可加入该组"
            >
              <button
                type="button"
                className="node-del"
                title="移除"
                onClick={(e) => {
                  e.stopPropagation();
                  onRemoveNode(node.name);
                }}
              >
                ×
              </button>
              <div className="node-card-name">
                {on && <span className="node-tick">✓</span>}
                {cc ? <span className={`fi fi-${cc} node-flag`} title={cc.toUpperCase()} /> : null}
                {meta.label}
              </div>
              <div className="node-card-foot">
                <span className="node-card-kind">{KIND_LABEL[node.kind] || node.kind || "?"}</span>
                {node.chain_entry && <span className="chain-badge mini">链</span>}
                <button
                  type="button"
                  className={`delay delay-action ${node.name in delays ? delayClass(ms) : ""}`}
                  disabled={testingNode === node.name}
                  onClick={(event) => {
                    event.stopPropagation();
                    onTestNode(node.name);
                  }}
                >
                  {testingNode === node.name
                    ? "测速中…"
                    : node.name in delays
                      ? ms == null
                        ? "超时·重测"
                        : `${ms} ms`
                      : "点击测速"}
                </button>
              </div>
            </div>
          );
        })}
      </div>
      )}
    </div>
  );
}

// ============================ 链式路径条 ============================

function ChainBar({
  activeNode,
  nodes,
  groups,
  picking,
  onSetChainEntry,
  onStartPick,
  onCancelPick,
}: {
  activeNode: ProxyNode | null;
  nodes: ProxyNode[];
  groups: ProxyGroup[];
  picking: boolean;
  onSetChainEntry: (exit: string, entry: string | null) => void;
  onStartPick: () => void;
  onCancelPick: () => void;
}) {
  if (!activeNode) {
    return null;
  }
  const entry = activeNode.chain_entry;
  const nodeCandidates = nodes.filter((n) => n.name !== activeNode.name);
  // 入口候选 = 其它节点 + 代理组(组当入口可自动选最优入口);组不能是出口自身。
  const groupCandidates = groups.filter((g) => g.name !== activeNode.name);
  const hasCandidates = nodeCandidates.length > 0 || groupCandidates.length > 0;

  return (
    <div className="chain-bar">
      <div className="chain-flow">
        {entry ? (
          <>
            <span className="chain-pill entry">{entry}</span>
            <span className="chain-arrow">→</span>
            <span className="chain-pill exit">{activeNode.name}</span>
            <span className="chain-tag">链式出口</span>
          </>
        ) : (
          <>
            <span className="chain-pill direct">直连</span>
            <span className="chain-arrow">→</span>
            <span className="chain-pill exit">{activeNode.name}</span>
            <span className="chain-tag">当前出口</span>
          </>
        )}
      </div>
      <div className="chain-ctl">
        {entry && (
          <button
            type="button"
            className="chain-btn ghost"
            onClick={() => onSetChainEntry(activeNode.name, null)}
          >
            取消链式
          </button>
        )}
        {!picking ? (
          <button type="button" className="chain-btn" onClick={onStartPick} disabled={!hasCandidates}>
            {entry ? "更换入口" : "＋ 加入口"}
          </button>
        ) : (
          <button type="button" className="chain-btn ghost" onClick={onCancelPick}>
            取消
          </button>
        )}
      </div>
      {picking && (
        <div className="chain-picker">
          <div className="chain-picker-title">
            选一个「入口」(流量先经它再到 {activeNode.name});选代理组则自动挑最优入口。
          </div>
          <div className="chain-picker-list">
            {groupCandidates.map((g) => (
              <button
                type="button"
                key={`g:${g.name}`}
                className={`chain-pick-item group${entry === g.name ? " on" : ""}`}
                onClick={() => {
                  onSetChainEntry(activeNode.name, g.name);
                  onCancelPick();
                }}
              >
                {g.name}
                <span className="chain-pick-kind">{g.kind === "url-test" ? "组·自动" : "组·手动"}</span>
              </button>
            ))}
            {nodeCandidates.map((n) => (
              <button
                type="button"
                key={n.name}
                className={`chain-pick-item${entry === n.name ? " on" : ""}`}
                onClick={() => {
                  onSetChainEntry(activeNode.name, n.name);
                  onCancelPick();
                }}
              >
                {n.name}
                <span className="chain-pick-kind">{KIND_LABEL[n.kind] || n.kind}</span>
              </button>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function EgressLine({ egress, onClose }: { egress: EgressView; onClose: () => void }) {
  return (
    <div className="egress-line-top" onClick={onClose} title="点击关闭">
      {egress.error ? (
        <span className="egress-err-inline">出口测试失败:{egress.error}</span>
      ) : (
        <span className="egress-line-text">
          <span className="egress-dot" /> 当前出口 <b>{egress.ip || "?"}</b>
          <span className="egress-sep">·</span>
          {egress.country} {egress.city}
          <span className="egress-sep">·</span>
          {egress.isp || "?"}
          <span className="egress-via">经由 {egress.via}</span>
        </span>
      )}
    </div>
  );
}

// ============================ 弹窗:添加订阅 ============================

function AddSubModal({
  busy,
  onClose,
  onAdd,
}: {
  busy: boolean;
  onClose: () => void;
  onAdd: (name: string, url: string) => void;
}) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [err, setErr] = useState("");

  const submit = () => {
    const u = url.trim();
    if (!/^https?:\/\//.test(u)) {
      setErr("请填 http(s):// 开头的订阅链接");
      return;
    }
    onAdd(name.trim() || hostOf(u), u);
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span className="modal-title">添加订阅</span>
          <button type="button" className="detail-close" onClick={onClose}>×</button>
        </div>
        <div className="modal-body">
          {err && <div className="error inline">{err}</div>}
          <div className="add-form">
            <Row label="订阅名">
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="可留空,自动用域名" />
            </Row>
            <Row label="订阅链接">
              <input
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://example.com/sub?token=..."
              />
            </Row>
            <button type="button" className="s-btn primary" disabled={busy} onClick={submit}>
              {busy ? "拉取中…" : "拉取并导入"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ============================ 弹窗:添加手动节点 ============================

function AddNodeModal({
  onClose,
  onAdd,
}: {
  onClose: () => void;
  onAdd: (nodes: ProxyNode[]) => void;
}) {
  const [tab, setTab] = useState<"form" | "paste">("form");
  const [form, setForm] = useState({
    name: "",
    kind: "trojan",
    server: "",
    port: "443",
    password: "",
    username: "",
    sni: "",
    skip_verify: true,
  });
  const [paste, setPaste] = useState("");
  const [error, setError] = useState("");

  const submitForm = () => {
    if (!form.server.trim() || !Number(form.port)) {
      setError("请填服务器和端口");
      return;
    }
    const node: ProxyNode = {
      name: form.name.trim() || `${form.kind}-${form.server.trim()}`,
      kind: form.kind,
      server: form.server.trim(),
      port: Number(form.port),
      chain_entry: null,
      sub: null,
    };
    // 协议字段按需平铺(空值不写,避免给内核传 null)。
    if (form.password) node.password = form.password;
    if (form.username) node.username = form.username;
    if (form.sni.trim()) node.sni = form.sni.trim();
    if (form.kind === "trojan") node.skip_verify = form.skip_verify;
    onAdd([node]);
  };

  const submitPaste = async () => {
    try {
      const parsed = await invoke<ProxyNode[]>("parse_clash_text", { text: paste });
      if (parsed.length === 0) {
        setError("没解析到节点 —— 请粘贴 Clash 节点(含 type/server/port)");
        return;
      }
      onAdd(parsed);
    } catch (reason) {
      setError(`解析失败:${String(reason)}`);
    }
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(event) => event.stopPropagation()}>
        <div className="modal-head">
          <span className="modal-title">添加节点</span>
          <button type="button" className="detail-close" onClick={onClose}>×</button>
        </div>
        <div className="segmented seg-modal">
          <button type="button" className={tab === "form" ? "on" : ""} onClick={() => setTab("form")}>
            手动填写
          </button>
          <button type="button" className={tab === "paste" ? "on" : ""} onClick={() => setTab("paste")}>
            粘贴 Clash
          </button>
        </div>
        <div className="modal-body">
          {error && <div className="error inline">{error}</div>}
          {tab === "form" ? (
            <div className="add-form">
              <Row label="名称">
                <input
                  value={form.name}
                  onChange={(event) => setForm({ ...form, name: event.target.value })}
                  placeholder="可留空自动生成"
                />
              </Row>
              <Row label="类型">
                <select
                  className="filter-select"
                  aria-label="节点类型"
                  value={form.kind}
                  onChange={(event) => setForm({ ...form, kind: event.target.value })}
                >
                  <option value="trojan">Trojan</option>
                  <option value="socks5">SOCKS5</option>
                  <option value="http">HTTP</option>
                </select>
              </Row>
              <Row label="服务器">
                <input
                  value={form.server}
                  onChange={(event) => setForm({ ...form, server: event.target.value })}
                  placeholder="example.com"
                />
              </Row>
              <Row label="端口">
                <input
                  value={form.port}
                  onChange={(event) => setForm({ ...form, port: event.target.value })}
                  placeholder="443"
                />
              </Row>
              {form.kind === "trojan" ? (
                <>
                  <Row label="密码">
                    <input
                      type="password"
                      aria-label="Trojan 密码"
                      value={form.password}
                      onChange={(event) => setForm({ ...form, password: event.target.value })}
                    />
                  </Row>
                  <Row label="SNI">
                    <input
                      value={form.sni}
                      onChange={(event) => setForm({ ...form, sni: event.target.value })}
                      placeholder="默认用服务器域名"
                    />
                  </Row>
                  <div className="toggle-row compact">
                    <div className="fp-toggle-body">
                      <span className="fp-toggle-title">允许不安全证书</span>
                    </div>
                    <IosSwitch
                      on={form.skip_verify}
                      onChange={(skipVerify) => setForm({ ...form, skip_verify: skipVerify })}
                    />
                  </div>
                </>
              ) : (
                <>
                  <Row label="用户名(可选)">
                    <input
                      value={form.username}
                      onChange={(event) => setForm({ ...form, username: event.target.value })}
                      placeholder="免认证留空"
                    />
                  </Row>
                  <Row label="密码(可选)">
                    <input
                      type="password"
                      aria-label="密码"
                      value={form.password}
                      onChange={(event) => setForm({ ...form, password: event.target.value })}
                    />
                  </Row>
                </>
              )}
              <button type="button" className="s-btn primary" onClick={submitForm}>
                添加节点
              </button>
            </div>
          ) : (
            <div className="add-form">
              <textarea
                className="node-paste"
                spellCheck={false}
                placeholder={
                  "粘贴 Clash 节点(可多个):\n- type: trojan\n  name: 节点名\n  server: example.com\n  port: 443\n  password: ****\n  sni: example.com\n  skip-cert-verify: true"
                }
                value={paste}
                onChange={(event) => setPaste(event.target.value)}
              />
              <button type="button" className="s-btn primary" onClick={submitPaste}>
                解析并导入
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="add-row">
      <span className="add-row-label">{label}</span>
      {children}
    </label>
  );
}

// ============================ 弹窗:新建代理组 ============================

function AddGroupModal({
  taken,
  onClose,
  onAdd,
}: {
  taken: string[];
  onClose: () => void;
  onAdd: (name: string, kind: "select" | "url-test") => void;
}) {
  const [name, setName] = useState("");
  const [kind, setKind] = useState<"select" | "url-test">("url-test");
  const [err, setErr] = useState("");

  const submit = () => {
    const n = name.trim();
    if (!n) {
      setErr("请填组名");
      return;
    }
    if (taken.includes(n)) {
      setErr(`「${n}」与已有组或节点重名`);
      return;
    }
    if (["DIRECT", "REJECT", "GLOBAL", "PASS", "COMPATIBLE"].includes(n.toUpperCase())) {
      setErr(`「${n}」是保留名,换一个`);
      return;
    }
    onAdd(n, kind);
  };

  return (
    <div className="modal-overlay" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <span className="modal-title">新建代理组</span>
          <button type="button" className="detail-close" onClick={onClose}>×</button>
        </div>
        <div className="modal-body">
          {err && <div className="error inline">{err}</div>}
          <div className="add-form">
            <Row label="组名">
              <input value={name} onChange={(e) => setName(e.target.value)} placeholder="如:香港自动 / 备用" />
            </Row>
            <Row label="选路方式">
              <div className="segmented seg-modal">
                <button
                  type="button"
                  className={kind === "url-test" ? "on" : ""}
                  onClick={() => setKind("url-test")}
                >
                  自动 · 按延迟
                </button>
                <button
                  type="button"
                  className={kind === "select" ? "on" : ""}
                  onClick={() => setKind("select")}
                >
                  手动选成员
                </button>
              </div>
            </Row>
            <div className="add-hint">
              {kind === "url-test"
                ? "组内按延迟+可用性自动选最优,节点挂了自动切换。"
                : "手动指定组内当前出口成员,随时点成员切换。"}
              建组后把节点拖进来即可。
            </div>
            <button type="button" className="s-btn primary" onClick={submit}>
              创建
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

