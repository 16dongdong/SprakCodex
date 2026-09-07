"use client";

import type { MessageCatalog } from "../types";

export const KO_SKILLS_MESSAGES: MessageCatalog = {
  "Skills 管理": "Skills 관리",
  "扫描并管理 codexmanager-service 主机上的 Codex Skills。":
    "codexmanager-service 호스트의 Codex Skills를 검색하고 관리합니다.",
  "分别管理独立 Skills 与 Codex 原生插件。":
    "독립형 Skills와 Codex 기본 플러그인을 각각 관리합니다.",
  "独立 Skills": "독립형 Skills",
  "Codex 插件": "Codex 플러그인",
  "系统内置 · 只读": "시스템 내장 · 읽기 전용",
  用户安装: "사용자 설치",
  系统只读: "시스템 읽기 전용",
  配置无效: "잘못된 매니페스트",
  展开描述: "설명 펼치기",
  收起描述: "설명 접기",
  "由 Codex 管理": "Codex에서 관리",
  不可安全删除: "안전하게 삭제할 수 없음",
  "Skill ZIP 已安装": "Skill ZIP을 설치했습니다",
  "Skill 目录已导入": "Skill 디렉터리를 가져왔습니다",
  "Skill 已删除": "Skill을 삭제했습니다",
  "请选择 ZIP 文件": "ZIP 파일을 선택하세요",
  "ZIP 文件不能超过 {size}": "ZIP 파일은 {size}를 초과할 수 없습니다",
  请输入服务主机上的绝对路径: "서비스 호스트의 절대 경로를 입력하세요",
  导入已有目录: "기존 디렉터리 가져오기",
  "安装 ZIP": "ZIP 설치",
  服务主机文件系统: "서비스 호스트 파일 시스템",
  "这里的安装、导入和删除都发生在 codexmanager-service 所在主机，不是浏览器所在设备。":
    "설치, 가져오기, 삭제는 브라우저 장치가 아닌 codexmanager-service 호스트에서 실행됩니다.",
  "搜索名称、描述或目录": "이름, 설명 또는 디렉터리 검색",
  "当前无法读取 Skills": "현재 Skills를 읽을 수 없습니다",
  "请确认管理 RPC 可用并已连接 codexmanager-service。":
    "관리 RPC를 사용할 수 있고 codexmanager-service가 연결되어 있는지 확인하세요.",
  "Skills 加载失败": "Skills를 불러오지 못했습니다",
  "没有匹配的 Skill": "일치하는 Skill 없음",
  "尚未发现 Skill": "발견된 Skill 없음",
  "请调整搜索条件。": "검색 조건을 조정하세요.",
  "可以安装一个 ZIP，或从服务主机导入已有目录。":
    "ZIP을 설치하거나 서비스 호스트의 기존 디렉터리를 가져올 수 있습니다.",
  "导入已有 Skill 目录": "기존 Skill 디렉터리 가져오기",
  "输入 codexmanager-service 主机上的绝对路径。目录根部必须包含 SKILL.md。":
    "codexmanager-service 호스트의 절대 경로를 입력하세요. 디렉터리 루트에 SKILL.md가 있어야 합니다.",
  服务主机绝对路径: "서비스 호스트 절대 경로",
  导入: "가져오기",
  "例如 /opt/codex-skills/my-skill": "예: /opt/codex-skills/my-skill",
  "删除 Skill": "Skill 삭제",
  "将从服务主机永久删除“{name}”目录。此操作不可撤销。":
    "서비스 호스트에서 ‘{name}’ 디렉터리를 영구 삭제합니다. 이 작업은 되돌릴 수 없습니다.",
  确认删除: "삭제",
  "Skills 市场": "Skills 마켓",
  "Codex Skills 市场": "Codex Skills 마켓",
  "Codex 插件市场": "Codex 플러그인 마켓",
  "通过 Codex 原生 Marketplace 安装完整插件，只展示包含标准 SKILL.md 的插件。":
    "Codex 기본 Marketplace를 통해 전체 플러그인을 설치합니다. 표준 SKILL.md가 포함된 플러그인만 표시합니다.",
  "插件中的 Skills 会随完整插件一起安装，不能在这里单独安装。":
    "플러그인에 포함된 Skills는 전체 플러그인과 함께 설치되며 여기에서 개별적으로 설치할 수 없습니다.",
  "包含 {count} 个 Codex Skills": "Codex Skills {count}개 포함",
  "收起 Skill 清单": "Skill 목록 접기",
  "查看全部 {count} 个 Skills": "Skills {count}개 모두 보기",
  "已由 Codex 安装": "Codex에서 설치됨",
  "已安装（未启用）": "설치됨(비활성화)",
  已安装但未启用: "설치되었지만 비활성화됨",
  安装完整插件: "전체 플러그인 설치",
  "Codex Marketplace 已导入": "Codex Marketplace를 가져왔습니다",
  "导入 Marketplace 失败": "Marketplace를 가져오지 못했습니다",
  "Marketplace 已刷新": "Marketplace를 새로 고쳤습니다",
  "刷新 Marketplace 失败": "Marketplace를 새로 고치지 못했습니다",
  "插件已安装，新建 Codex 会话后生效":
    "플러그인을 설치했습니다. 새 Codex 세션에서 적용됩니다.",
  安装插件失败: "플러그인을 설치하지 못했습니다",
  "请输入 GitHub 仓库": "GitHub 저장소를 입력하세요",
  "GitHub 仓库，例如 openai/role-specific-plugins":
    "GitHub 저장소(예: openai/role-specific-plugins)",
  "GitHub Marketplace 仓库": "GitHub Marketplace 저장소",
  "分支或标签（可选）": "브랜치 또는 태그(선택 사항)",
  "例如 main": "예: main",
  导入市场: "마켓 가져오기",
  已连接市场: "연결된 마켓",
  "搜索插件、市场或 Skill": "플러그인, 마켓 또는 Skill 검색",
  "{count} 个兼容插件": "호환 플러그인 {count}개",
  "已安装 {count}": "설치됨 {count}개",
  刷新市场: "마켓 새로 고침",
  "Marketplace 加载失败": "Marketplace를 불러오지 못했습니다",
  "当前无法读取 Skills 市场": "현재 Skills Marketplace를 읽을 수 없습니다",
  "当前无法读取插件市场": "현재 플러그인 Marketplace를 읽을 수 없습니다",
  "当前 Codex CLI 不支持 Skills 市场":
    "현재 Codex CLI는 Skills 마켓을 지원하지 않습니다",
  "当前 Codex CLI 不支持插件市场":
    "현재 Codex CLI는 플러그인 마켓을 지원하지 않습니다",
  "请在 codexmanager-service 主机安装或升级支持 plugin 命令的 Codex CLI。":
    "codexmanager-service 호스트에서 plugin 명령을 지원하는 Codex CLI를 설치하거나 업그레이드하세요.",
  "没有匹配的 Marketplace 插件": "일치하는 Marketplace 플러그인 없음",
  "没有发现兼容的 Codex 插件": "호환되는 Codex 플러그인을 찾지 못했습니다",
  "导入 GitHub Marketplace；不含 Codex 插件清单或标准 SKILL.md 的插件会被忽略。":
    "GitHub Marketplace를 가져오세요. Codex 매니페스트 또는 표준 SKILL.md가 없는 플러그인은 무시됩니다.",
  "安装完整 Codex 插件": "전체 Codex 플러그인 설치",
  "将安装“{name}”完整插件（市场：{marketplace}；来源：{source}），其中包含 {count} 个 Skills，也可能包含 MCP、Hooks、Apps 或脚本。仅在信任来源时继续。":
    "전체 ‘{name}’ 플러그인을 설치합니다(마켓: {marketplace}, 출처: {source}). Skills {count}개가 포함되어 있으며 MCP 서버, 훅, 앱 또는 스크립트도 포함될 수 있습니다. 출처를 신뢰하는 경우에만 계속하세요.",
  确认安装插件: "플러그인 설치",
  "Skills 与插件": "Skills 및 플러그인",
  "安装独立 Skills，或管理 Codex 原生插件。":
    "독립형 Skills를 설치하거나 Codex 네이티브 플러그인을 관리합니다.",
  "Skills 安装": "Skills 설치",
  "Codex 插件安装": "Codex 플러그인 설치",
  "安装独立 Skills": "독립형 Skills 설치",
  "从技能仓库或 skills.sh 发现并安装，也可以继续使用本地 ZIP 或目录。":
    "Skill 저장소 또는 skills.sh에서 찾아 설치하거나 로컬 ZIP 및 디렉터리를 사용할 수 있습니다.",
  管理仓库: "저장소 관리",
  导入目录: "디렉터리 가져오기",
  技能仓库: "Skill 저장소",
  可安装: "설치 가능",
  "Skill 已安装": "Skill이 설치되었습니다",
  "Skill 已卸载": "Skill이 제거되었습니다",
  卸载失败: "제거 실패",
  打开来源失败: "소스를 열지 못했습니다",
  技能目录加载失败: "Skill 카탈로그를 불러오지 못했습니다",
  "请调整搜索或筛选条件。": "검색어나 필터를 조정하세요.",
  "搜索 Skill、描述或作者": "Skill, 설명 또는 작성자 검색",
  全部仓库: "모든 저장소",
  "显示 {count} 个 Skills，来自 {repositories} 个仓库":
    "{repositories}개 저장소의 Skills {count}개 표시",
  "搜索 skills.sh": "skills.sh 검색",
  "输入至少 2 个字符搜索公开 Skills。":
    "공개 Skills를 검색하려면 2자 이상 입력하세요.",
  "skills.sh 提供公开 Skills 索引；安装前请核对来源。":
    "skills.sh는 공개 Skills 색인을 제공합니다. 설치 전에 소스를 확인하세요.",
  "搜索已安装 Skills": "설치된 Skills 검색",
  "尚未安装 Skill": "설치된 Skill 없음",
  "从技能仓库、skills.sh、本地 ZIP 或目录安装。":
    "Skill 저장소, skills.sh, 로컬 ZIP 또는 디렉터리에서 설치하세요.",
  "卸载 Skill": "Skill 제거",
  "将从服务主机删除“{name}”。仓库记录仍会保留，可随时重新安装。":
    "서비스 호스트에서 “{name}”을 제거합니다. 저장소 기록은 남아 있어 언제든 다시 설치할 수 있습니다.",
  确认卸载: "제거",
  管理技能仓库: "Skill 저장소 관리",
  "添加公共 GitHub 仓库并同步其中的 SKILL.md。删除仓库不会删除已安装的 Skills。":
    "공개 GitHub 저장소를 추가하고 SKILL.md를 동기화합니다. 저장소를 제거해도 설치된 Skills는 삭제되지 않습니다.",
  "GitHub 仓库 URL": "GitHub 저장소 URL",
  分支或标签: "브랜치 또는 태그",
  默认分支: "기본 브랜치",
  添加仓库: "저장소 추가",
  已连接仓库: "연결된 저장소",
  "共 {count} 个仓库": "저장소 {count}개",
  全部刷新: "모두 새로 고침",
  "请先连接 codexmanager-service。": "먼저 codexmanager-service에 연결하세요.",
  尚未添加技能仓库: "추가된 Skill 저장소 없음",
  内置: "내장",
  同步失败: "동기화 실패",
  已同步: "동기화됨",
  等待同步: "동기화 대기",
  "发现 {count} 个 Skills": "Skills {count}개 발견",
  最近同步: "최근 동기화",
  "确定删除仓库“{name}”？已安装的 Skills 会保留。":
    "저장소 “{name}”을 제거할까요? 설치된 Skills는 유지됩니다.",
  技能仓库已添加: "Skill 저장소가 추가되었습니다",
  添加仓库失败: "저장소 추가 실패",
  技能仓库已刷新: "Skill 저장소가 새로 고쳐졌습니다",
  刷新仓库失败: "저장소 새로 고침 실패",
  "技能仓库已删除，已安装的 Skills 不受影响":
    "Skill 저장소가 제거되었으며 설치된 Skills는 변경되지 않았습니다",
  删除仓库失败: "저장소 제거 실패",
  "{count} 次安装": "설치 {count}회",
  初始化技能仓库失败: "Skill 저장소 초기화 실패",
  "首次进入，正在后台同步技能仓库…":
    "첫 방문을 위해 백그라운드에서 Skill 저장소를 동기화하는 중…",
};
