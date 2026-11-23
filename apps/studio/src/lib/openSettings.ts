/**
 * Open the Settings page from anywhere in the Studio, optionally at one of its sections.
 *
 * A window event rather than a prop threaded down from the shell: the tool asking (the Bike
 * tab's "needs Blender" line, say) sits several components below `App`, which owns the view,
 * and none of the layers in between have any business knowing about Settings. `App` listens
 * and switches to Settings; the section is in the event's `detail` for the page to scroll to.
 */
export const OPEN_SETTINGS_EVENT = "studio:open-settings";

export type SettingsSection = "paths";

export function openSettings(section?: SettingsSection): void {
  window.dispatchEvent(new CustomEvent(OPEN_SETTINGS_EVENT, { detail: { section } }));
}
