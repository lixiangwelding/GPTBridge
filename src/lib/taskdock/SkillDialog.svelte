<script lang="ts">
  import Dialog from "./Dialog.svelte";
  import { readSkill } from "./api";
  import type { Skill, SkillContent } from "./types";
  let { selected, onClose }: { selected: { workspaceId: string; skill: Skill } | null; onClose: () => void } = $props();
  let content = $state<SkillContent | null>(null), error = $state(""), busy = $state(false);
  let token = 0;
  $effect(() => { void selected; content = null; error = ""; token++; if(selected)void load(0,null); });
  async function load(offset = 0, sha: string | null = null) {
    if (!selected) return; const current = ++token, target = selected; busy = true;
    try { const result = await readSkill(target.workspaceId,target.skill.skill_id,offset,sha); if(current===token){content=result;error="";} }
    catch(e){if(current===token)error=String(e);}finally{if(current===token)busy=false;}
  }
</script>
<Dialog open={!!selected} title={selected?.skill.name || "技能详情"} onClose={onClose} wide>
  {#if selected}<p>{selected.skill.description}</p><span class="td-pill">{selected.skill.source_manual_only ? "源文件禁止自动调用" : selected.skill.user_disabled ? "自动匹配已停用" : "可自动匹配"}</span>{/if}
  {#if error}<div class="td-error" role="alert">{error}</div>{/if}
  {#if content}<dl class="td-keyval" style="margin-top:16px"><dt>真实来源</dt><dd class="td-mono">{content.source_path}</dd><dt>内容校验</dt><dd class="td-mono">{content.sha256}</dd><dt>当前范围</dt><dd>第 {content.start_line}–{content.end_line} 行 · 文件共 {content.total_bytes} 字节</dd></dl>{/if}
  <pre class="td-code">{busy ? "读取技能源文件…" : content?.content || "尚未读取内容。"}</pre>
  <div class="td-note">这里只读取技能原文，不运行其中的脚本。自动匹配开关不会扩大执行权限，源文件的限制始终保留。</div>
  {#snippet footer()}<button class="td-button" disabled={busy} onclick={() => load(0,null)}>重新读取</button><button class="td-button" disabled={busy || content?.next_offset == null} onclick={() => content && load(content.next_offset || 0,content.sha256)}>下一段</button><button class="td-button primary" onclick={onClose}>关闭</button>{/snippet}
</Dialog>
