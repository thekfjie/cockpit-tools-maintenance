from pathlib import Path
import subprocess

root = Path.cwd()
for relative in [
    "src-tauri/src/modules/codex_local_project_recovery/account_state.rs",
    "src-tauri/src/modules/codex_local_project_recovery/account_state_tests.rs",
]:
    subprocess.run(["rustfmt", "--edition", "2021", "--config", "skip_children=true", relative], check=True)
for relative, marker in [
    ("src-tauri/src/modules/codex_account_projection.rs", "/// Derive the Desktop renderer owner"),
    ("src-tauri/src/modules/codex_session_visibility_projects.rs", "/// Prepare account-scoped local groups"),
]:
    path = root / relative
    text = path.read_text()
    position = text.index(marker)
    result = subprocess.run(["rustfmt", "--edition", "2021", "--config", "skip_children=true", "--emit", "stdout"], input=text[position:], text=True, capture_output=True, check=True)
    path.write_text(text[:position].rstrip() + "\\n\\n" + result.stdout)
patch = subprocess.run(["git", "diff", "--", "src-tauri/src"], capture_output=True, check=True).stdout
Path("pr-formatting.patch").write_bytes(patch)
print("Formatting patch bytes:", len(patch))
