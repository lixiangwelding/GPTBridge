import { writable } from "svelte/store";
import { workspaces } from "$lib/stores/app";
import { getAppInfo, getProjects } from "./api";
import type { AppInfo } from "./types";

export const projectLoading = writable(false);
export const projectError = writable("");
export const addProjectOpen = writable(false);
export const newTaskOpen = writable(false);
export const connectionProject = writable<string | null>(null);
export const compact = writable(false);
export const appInfo = writable<AppInfo | null>(null);
export const taskCreated = writable<{ workspaceId: string; taskId: string } | null>(null);
let loading: Promise<void> | null = null;

export function refreshProjects(): Promise<void> {
  if (loading) return loading;
  projectLoading.set(true);
  loading = (async () => {
    try { workspaces.set(await getProjects()); projectError.set(""); }
    catch (error) { projectError.set(String(error)); }
    finally { projectLoading.set(false); loading = null; }
  })();
  return loading;
}
export async function initialize(): Promise<void> {
  try { compact.set(localStorage.getItem("taskdock:compact") === "1"); } catch { /* optional presentation preference */ }
  await refreshProjects();
  try { appInfo.set(await getAppInfo()); } catch { appInfo.set(null); }
}
export function setCompact(value: boolean): void {
  compact.set(value);
  try { localStorage.setItem("taskdock:compact", value ? "1" : "0"); } catch { /* session-only preference remains usable */ }
}
