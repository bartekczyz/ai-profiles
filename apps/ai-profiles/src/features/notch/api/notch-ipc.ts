import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

/** What the notch panel is doing. `hidden` means no display has a notch. */
type NotchPhase = 'hidden' | 'collapsed' | 'expanded'

/** The panel geometry the shell reports, in logical points. */
export type NotchState = {
  phase: NotchPhase
  /** The notch width, which the panel leaves empty. */
  notch: number
  /** The notch height: the collapsed row. */
  height: number
  /** The drawable strip either side of the notch. */
  wing: number
  /** The transparent gutter outside the panel, where its top corners curve out. */
  fillet: number
  /** The black panel's width, without the gutters. */
  bodyWidth: number
  /** The most the open panel can grow below the notch row. */
  bodyMaxHeight: number
}

export const notchHidden: NotchState = {
  phase: 'hidden',
  notch: 0,
  height: 0,
  wing: 0,
  fillet: 0,
  bodyWidth: 0,
  bodyMaxHeight: 0,
}

export function getNotchState(): Promise<NotchState> {
  return invoke<NotchState>('notch_state')
}

/** Follow the panel as it opens, folds, and moves with the displays. */
export function onNotchState(handler: (state: NotchState) => void): Promise<() => void> {
  return listen<NotchState>('notch:state', (event) => handler(event.payload))
}

/** Report the height of the meters, so the shell sizes the open panel to them. */
export function setNotchContentHeight(height: number): Promise<void> {
  return invoke('notch_set_content_height', { height })
}

/** A profile's limit crossed a usage alert threshold. */
export type UsageAlert = {
  profileId: string
  window: 'weekly' | 'session'
  /** The window's length in minutes: 10080 for a week. */
  windowMinutes: number
  threshold: number
  used: number
  /** The share of the window gone, 0-100. */
  elapsed: number
  /** Usage is ahead of a straight-line pace through the window. */
  ahead: boolean
}

/** Follow the usage alerts that drop the panel open. */
export function onNotchAlert(handler: (alert: UsageAlert) => void): Promise<() => void> {
  return listen<UsageAlert>('notch:alert', (event) => handler(event.payload))
}

/** Report the width the folded profile summary needs either side of the notch. */
export function setNotchWingWidth(width: number): Promise<void> {
  return invoke('notch_set_wing_width', { width })
}

/** Show profile `id` in the main window. */
export function openProfileFromNotch(id: string): Promise<void> {
  return invoke('notch_open_profile', { id })
}

/** The main window's side: the notch asked for profile `id`. */
export function onOpenProfileFromNotch(handler: (id: string) => void): Promise<() => void> {
  return listen<string>('notch:open-profile', (event) => handler(event.payload))
}
