import os
import json
import argparse
import sys
import subprocess
import shutil
import re
import urllib.request
import logging
from datetime import datetime

logging.basicConfig(
    level=logging.INFO,
    format='[%(asctime)s] %(levelname)s: %(message)s',
    datefmt='%H:%M:%S',
    stream=sys.stdout
)

CONFIG_PATH = "configs/projects.json"
UPSTREAM_PATH = "configs/upstream_commits.json"
TEMPLATES_DIR = "templates"
WORKSPACE_DIR = "kernel_workspace"

KSU_CONFIG = {
    "ksu": {
        "repo": "https://github.com/tiann/KernelSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/tiann/KernelSU/main/kernel/setup.sh",
        "setup_args": ["main"]
    },
    "mksu": {
        "repo": "https://github.com/5ec1cff/KernelSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/5ec1cff/KernelSU/main/kernel/setup.sh",
        "setup_args": ["main"]
    },
    "resukisu": {
        "repo": "https://github.com/ReSukiSU/ReSukiSU.git",
        "branch": "main",
        "setup_url": "https://raw.githubusercontent.com/ReSukiSU/ReSukiSU/main/kernel/setup.sh",
        "setup_args": ["builtin"]
    }
}

def run_cmd(cmd, cwd=None, check=True, capture=False):
    cmd_str = " ".join(cmd)
    if not capture:
        logging.info(f"Exec: {cmd_str}")
    try:
        if capture:
            result = subprocess.run(
                cmd,
                cwd=cwd,
                check=check,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True
            )
            return result.stdout.strip()
        else:
            subprocess.run(cmd, cwd=cwd, check=check)
            return None
    except subprocess.CalledProcessError as e:
        if check:
            logging.error(f"Command failed: {cmd_str}")
            if capture:
                logging.error(f"Stderr: {e.stderr}")
            raise e
        return None

def load_json(path):
    if not os.path.exists(path):
        return {}
    with open(path, 'r') as f:
        return json.load(f)

def save_json(path, data):
    with open(path, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')

def set_github_env(key, value):
    if "GITHUB_ENV" in os.environ:
        with open(os.environ["GITHUB_ENV"], "a") as f:
            f.write(f"{key}={value}\n")
    else:
        print(f"EXPORT {key}={value}")

def get_project_env(project_key):
    data = load_json(CONFIG_PATH)
    if project_key not in data:
        sys.exit(1)

    proj = data[project_key]

    envs = {
        "PROJECT_REPO": proj.get("repo"),
        "PROJECT_DEFCONFIG": proj.get("defconfig"),
        "PROJECT_LOCALVERSION_BASE": proj.get("localversion_base"),
        "PROJECT_LTO": proj.get("lto", ""),
        "PROJECT_TOOLCHAIN_PREFIX": proj.get("toolchain_path_prefix", ""),
        "PROJECT_ZIP_NAME_PREFIX": proj.get("zip_name_prefix", "Kernel"),
        "PROJECT_AK3_REPO": proj.get("anykernel_repo"),
        "PROJECT_AK3_BRANCH": proj.get("anykernel_branch"),
        "PROJECT_VERSION_METHOD": proj.get("version_method", "param"),
        "PROJECT_EXTRA_HOST_ENV": str(proj.get("extra_host_env", False)).lower(),
    }

    envs["PROJECT_TOOLCHAIN_EXPORTS"] = json.dumps(proj.get("toolchain_path_exports", []))
    envs["PROJECT_DISABLE_SECURITY"] = json.dumps(proj.get("disable_security", []))

    if "toolchain_urls" in proj:
        envs["PROJECT_TOOLCHAIN_URLS"] = json.dumps(proj["toolchain_urls"])

    for k, v in envs.items():
        set_github_env(k, v)

def generate_build_meta(project_key, branch_name):
    data = load_json(CONFIG_PATH)
    if project_key not in data:
        sys.exit(1)

    proj = data[project_key]
    zip_prefix = proj.get("zip_name_prefix", "Kernel")
    localversion_base = proj.get("localversion_base", "")

    suffix_map = {
        "main": "LKM",
        "lkm": "LKM",
        "ksu": "KSU",
        "mksu": "MKSU",
        "resukisu": "ReSuki",
        "sukisuultra": "ReSuki"
    }

    variant_suffix = suffix_map.get(branch_name, branch_name.upper())
    date_str = datetime.now().strftime("%Y%m%d")

    final_localversion = f"{localversion_base}-{variant_suffix}"

    release_tag = f"{zip_prefix}-{variant_suffix}-{date_str}"

    final_zip_name = f"{zip_prefix}-{variant_suffix}-{date_str}.zip"

    release_title = f"{zip_prefix} {variant_suffix} Build ({date_str})"

    set_github_env("BUILD_VARIANT_SUFFIX", variant_suffix)
    set_github_env("FINAL_LOCALVERSION", final_localversion)
    set_github_env("RELEASE_TAG", release_tag)
    set_github_env("FINAL_ZIP_NAME", final_zip_name)
    set_github_env("RELEASE_TITLE", release_title)

    logging.info(f"Meta Generated: Tag={release_tag}, Zip={final_zip_name}")

def generate_release_matrix(project_key, gh_token):
    data = load_json(CONFIG_PATH)
    if project_key not in data:
        return

    raw_supported = data[project_key].get("supported_ksu", [])
    supported = [x if x != 'sukisuultra' else 'resukisu' for x in raw_supported]
    branches = ["main"] + supported

    matrix = {"include": [{"branch": b} for b in branches]}

    if "GITHUB_OUTPUT" in os.environ:
        with open(os.environ["GITHUB_OUTPUT"], "a") as f:
            f.write(f"matrix={json.dumps(matrix)}\n")
    else:
        print(json.dumps(matrix))

def add_project(args):
    logging.info(f"Adding new project: {args.key}")
    data = load_json(CONFIG_PATH)

    new_proj = {
        "repo": args.repo,
        "defconfig": args.defconfig,
        "localversion_base": args.localversion,
        "anykernel_repo": args.ak3_repo,
        "anykernel_branch": args.ak3_branch,
        "zip_name_prefix": args.zip_name,
        "supported_ksu": ["resukisu", "mksu", "ksu"],
        "readme_placeholders": {
            "DEVICE_NAME_CN": args.device_cn,
            "DEVICE_NAME_EN": args.device_en
        }
    }

    if args.toolchain_prefix:
        new_proj["toolchain_path_prefix"] = args.toolchain_prefix

    data[args.key] = new_proj
    save_json(CONFIG_PATH, data)
    logging.info("Project added successfully.")

def process_readme(template_content, proj_config, repo_url, lang):
    content = template_content
    placeholders = proj_config.get("readme_placeholders", {})
    cn_name = placeholders.get("DEVICE_NAME_CN", "未知设备")
    en_name = placeholders.get("DEVICE_NAME_EN", "Unknown Device")
    localver = proj_config.get("localversion_base", "")

    replacements = {
        "__DEVICE_NAME_CN__": cn_name,
        "__DEVICE_NAME_EN__": en_name,
        "__PROJECT_REPO__": repo_url,
        "__LOCALVERSION_BASE__": localver
    }

    for k, v in replacements.items():
        content = content.replace(k, v)

    if lang == "zh-CN":
        content = re.sub(r'.*?', '', content, flags=re.DOTALL)
    elif lang == "en-US":
        content = re.sub(r'.*?', '', content, flags=re.DOTALL)

    content = re.sub(r'', '', content)
    return re.sub(r'\n{3,}', '\n\n', content).strip()

def setup_repos(args):
    logging.info("Starting batch repository setup...")
    data = load_json(CONFIG_PATH)
    token = args.token
    commit_msg = args.commit_message
    readme_lang = args.readme_language

    if not os.path.exists(WORKSPACE_DIR):
        os.makedirs(WORKSPACE_DIR)

    with open(os.path.join(TEMPLATES_DIR, "README.md.tpl"), "r") as f:
        readme_tpl = f.read()
    with open(os.path.join(TEMPLATES_DIR, "trigger-central-build.yml.tpl"), "r") as f:
        trigger_tpl = f.read()

    run_cmd(["git", "config", "--global", "user.name", "Kokuban-Bot"])
    run_cmd(["git", "config", "--global", "user.email", "bot@kokuban.dev"])

    for key, proj in data.items():
        if key.startswith("_"): continue
        repo_url = proj.get("repo")
        if not repo_url: continue

        logging.info(f"Processing project: {key} -> {repo_url}")
        target_dir = os.path.join(WORKSPACE_DIR, key)
        auth_url = f"https://{token}@github.com/{repo_url}.git" if token else f"https://github.com/{repo_url}.git"

        if os.path.exists(target_dir):
            shutil.rmtree(target_dir)

        run_cmd(["git", "clone", auth_url, target_dir])

        readme_content = process_readme(readme_tpl, proj, repo_url, readme_lang)
        target_branches = ["main", "ksu", "mksu", "resukisu"]

        remote_branches_raw = run_cmd(["git", "branch", "-r"], cwd=target_dir, capture=True)
        remote_branches = [b.strip().replace("origin/", "") for b in remote_branches_raw.splitlines()]

        for branch in target_branches:
            cwd = target_dir
            branch_exists = branch in remote_branches
            logging.info(f"  Configuring branch: {branch}")

            if branch == "resukisu" and not branch_exists and "sukisuultra" in remote_branches:
                run_cmd(["git", "checkout", "sukisuultra"], cwd=cwd)
                run_cmd(["git", "branch", "-m", "resukisu"], cwd=cwd)
                run_cmd(["git", "push", "origin", "-u", "resukisu"], cwd=cwd)
                run_cmd(["git", "push", "origin", "--delete", "sukisuultra"], cwd=cwd)
                branch_exists = True
            elif branch_exists:
                run_cmd(["git", "checkout", branch], cwd=cwd)
            else:
                continue

            with open(os.path.join(cwd, "README.md"), "w") as f:
                f.write(readme_content)

            github_dir = os.path.join(cwd, ".github")
            workflows_dir = os.path.join(github_dir, "workflows")
            if not os.path.exists(github_dir): os.makedirs(github_dir)

            src_funding = os.path.join(".github", "FUNDING.yml")
            if os.path.exists(src_funding):
                shutil.copy(src_funding, os.path.join(github_dir, "FUNDING.yml"))

            if os.path.exists(workflows_dir): shutil.rmtree(workflows_dir)
            os.makedirs(workflows_dir)

            for old_file in ["build.sh", "build_kernel.sh", "update.sh", "update-kernelsu.yml"]:
                old_path = os.path.join(cwd, old_file)
                if os.path.exists(old_path): os.remove(old_path)

            trigger_content = trigger_tpl.replace("__PROJECT_KEY__", key)
            repo_owner = repo_url.split('/')[0] if '/' in repo_url else "YuzakiKokuban"
            trigger_content = trigger_content.replace("__REPO_OWNER__", repo_owner)

            with open(os.path.join(workflows_dir, "trigger-central-build.yml"), "w") as f:
                f.write(trigger_content)

            src_gitignore = os.path.join("configs", "universal.gitignore")
            if os.path.exists(src_gitignore):
                shutil.copy(src_gitignore, os.path.join(cwd, ".gitignore"))

            run_cmd(["git", "add", "."], cwd=cwd)
            status = run_cmd(["git", "status", "--porcelain"], cwd=cwd, capture=True)
            if status:
                run_cmd(["git", "commit", "-m", f"{commit_msg} (branch: {branch})"], cwd=cwd)
                run_cmd(["git", "push", "origin", branch], cwd=cwd)

        if args.token:
            try:
                p = subprocess.Popen(
                    ["gh", "secret", "set", "CI_TOKEN"],
                    stdin=subprocess.PIPE,
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    cwd=target_dir,
                    text=True
                )
                stdout, stderr = p.communicate(input=token)
            except Exception:
                pass

            run_cmd(["gh", "api", "--method", "PATCH", f"repos/{repo_url}", "-f", "has_sponsorships=true", "--silent"], check=False)

def get_remote_head(repo_url, branch):
    logging.info(f"Checking remote head for {repo_url} ({branch})...")
    cmd = ["git", "ls-remote", repo_url, branch]
    output = run_cmd(cmd, capture=True)
    if output:
        return output.split()[0]
    return None

def watch_upstream(args):
    logging.info("Checking for KernelSU upstream updates...")
    track_data = load_json(UPSTREAM_PATH)
    projects_data = load_json(CONFIG_PATH)
    update_matrix = []

    track_data.pop("sukisuultra", None)

    for variant, config in KSU_CONFIG.items():
        latest_hash = get_remote_head(config["repo"], config["branch"])

        if not latest_hash:
            continue

        stored_hash = track_data.get(variant, "")

        if latest_hash != stored_hash:
            track_data[variant] = latest_hash

            for p_key, p_data in projects_data.items():
                if p_key.startswith("_"): continue
                supported = [x.replace("sukisuultra", "resukisu") for x in p_data.get("supported_ksu", [])]
                if variant in supported:
                    update_matrix.append({
                        "project": p_key,
                        "variant": variant,
                        "commit_id": latest_hash[:7]
                    })

    save_json(UPSTREAM_PATH, track_data)

    if "GITHUB_OUTPUT" in os.environ:
        with open(os.environ["GITHUB_OUTPUT"], "a") as f:
            f.write(f"matrix={json.dumps(update_matrix)}\n")
            f.write(f"found_updates={'true' if update_matrix else 'false'}\n")

def perform_update(args):
    token = args.token
    project_key = args.project
    variant = args.variant
    commit_id = args.commit_id

    variant = variant.replace("sukisuultra", "resukisu")

    data = load_json(CONFIG_PATH)
    if project_key not in data:
        sys.exit(1)

    repo_url = data[project_key]["repo"]
    target_dir = "temp_kernel"

    if os.path.exists(target_dir):
        shutil.rmtree(target_dir)

    auth_url = f"https://{token}@github.com/{repo_url}.git"

    run_cmd(["git", "clone", "--depth=1", "--branch", variant, auth_url, target_dir])

    with open(os.path.join(target_dir, "KERNELSU_VERSION.txt"), "w") as f:
        f.write(commit_id)

    src_gitignore = os.path.join("configs", "universal.gitignore")
    if os.path.exists(src_gitignore):
        shutil.copy(src_gitignore, os.path.join(target_dir, ".gitignore"))

    ksu_cfg = KSU_CONFIG.get(variant)
    if ksu_cfg:
        setup_script_path = os.path.join(target_dir, "setup.sh")

        try:
            with urllib.request.urlopen(ksu_cfg['setup_url']) as response, open(setup_script_path, 'wb') as out_file:
                shutil.copyfileobj(response, out_file)
        except Exception:
            sys.exit(1)

        cmd = ["bash", "setup.sh"] + ksu_cfg["setup_args"]
        run_cmd(cmd, cwd=target_dir)
        os.remove(setup_script_path)

    run_cmd(["git", "config", "user.name", "Kokuban-Bot"], cwd=target_dir)
    run_cmd(["git", "config", "user.email", "bot@kokuban.dev"], cwd=target_dir)

    run_cmd(["git", "add", "."], cwd=target_dir)
    if run_cmd(["git", "status", "--porcelain"], cwd=target_dir, capture=True):
        msg = f"ci: update {variant} to {commit_id}"
        run_cmd(["git", "commit", "-m", msg], cwd=target_dir)
        run_cmd(["git", "push"], cwd=target_dir)
    else:
        pass

    shutil.rmtree(target_dir)

def send_telegram_notify(args):
    import requests

    token = os.environ.get("TELEGRAM_BOT_TOKEN")
    if not token:
        logging.error("Missing TELEGRAM_BOT_TOKEN")
        sys.exit(1)

    tag_name = args.tag
    projects_data = load_json(CONFIG_PATH)
    global_config = projects_data.get("_globals", {})

    target_project = None

    for key, proj in projects_data.items():
        if key.startswith("_"): continue
        zip_prefix = proj.get("zip_name_prefix", "Kernel")
        if tag_name.startswith(zip_prefix):
            target_project = proj
            break

    if not target_project:
        logging.info(f"No project found for tag {tag_name}")
        return

    destinations = []
    
    default_chat = global_config.get("default_chat_id")
    if default_chat:
        destinations.append({"chat_id": default_chat})

    if "ReSuki" in tag_name:
        rs_chat = global_config.get("resukisu_chat_id")
        rs_topic = global_config.get("resukisu_topic_id")
        if rs_chat:
            destinations.append({"chat_id": rs_chat, "message_thread_id": rs_topic})

    if not destinations:
        logging.info("No destination found.")
        return

    out = run_cmd(["gh", "release", "view", tag_name, "--json", "assets,body,name,url"], capture=True)
    release_info = json.loads(out)

    msg = (
        f"📦 <b>New Build Released!</b>\n\n"
        f"🏷 <b>Tag</b>: <code>{tag_name}</code>\n"
        f"📝 <b>Title</b>: {release_info.get('name', 'Update')}\n"
        f"🔗 <a href='{release_info['url']}'>View on GitHub</a>"
    )

    for dest in destinations:
        send_url = f"https://api.telegram.org/bot{token}/sendMessage"
        payload = {
            "chat_id": dest["chat_id"],
            "text": msg,
            "parse_mode": "HTML",
            "disable_web_page_preview": True
        }
        if "message_thread_id" in dest:
            payload["message_thread_id"] = dest["message_thread_id"]
        
        try:
            requests.post(send_url, json=payload).raise_for_status()
        except Exception as e:
            logging.error(f"Failed to send msg to {dest['chat_id']}: {e}")

    assets = release_info.get("assets", [])
    if not assets:
        return

    for asset in assets:
        if asset["size"] > 50 * 1024 * 1024:
            logging.warning(f"Skipping {asset['name']} (too large)")
            continue

        logging.info(f"Downloading {asset['name']}...")
        run_cmd(["gh", "release", "download", tag_name, "-p", asset["name"]])
        
        try:
            for dest in destinations:
                logging.info(f"Uploading {asset['name']} to {dest['chat_id']}...")
                with open(asset["name"], 'rb') as f:
                    files = {'document': f}
                    data = {'chat_id': dest['chat_id'], 'caption': f"📄 <code>{asset['name']}</code>", 'parse_mode': 'HTML'}
                    if "message_thread_id" in dest:
                        data['message_thread_id'] = dest["message_thread_id"]

                    requests.post(
                        f"https://api.telegram.org/bot{token}/sendDocument",
                        data=data,
                        files=files
                    )
                    f.seek(0)
        finally:
            if os.path.exists(asset["name"]):
                os.remove(asset["name"])

    logging.info("Notification sent successfully.")

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command")

    p_parse = subparsers.add_parser("parse")
    p_parse.add_argument("--project", required=True)

    p_meta = subparsers.add_parser("meta")
    p_meta.add_argument("--project", required=True)
    p_meta.add_argument("--branch", required=True)

    p_matrix = subparsers.add_parser("matrix")
    p_matrix.add_argument("--project", required=True)
    p_matrix.add_argument("--token")

    p_add = subparsers.add_parser("add")
    p_add.add_argument("--key", required=True)
    p_add.add_argument("--repo", required=True)
    p_add.add_argument("--defconfig", required=True)
    p_add.add_argument("--localversion", required=True)
    p_add.add_argument("--device_cn", default="未知设备")
    p_add.add_argument("--device_en", default="Unknown Device")
    p_add.add_argument("--ak3_repo", default="https://github.com/YuzakiKokuban/AnyKernel3.git")
    p_add.add_argument("--ak3_branch", default="master")
    p_add.add_argument("--zip_name", default="Kernel")
    p_add.add_argument("--toolchain_prefix", default="")

    p_setup = subparsers.add_parser("setup")
    p_setup.add_argument("--token")
    p_setup.add_argument("--commit_message", default="[skip ci] ci: Sync central CI files")
    p_setup.add_argument("--readme_language", default="both", choices=["both", "zh-CN", "en-US"])

    p_watch = subparsers.add_parser("watch")

    p_update = subparsers.add_parser("update")
    p_update.add_argument("--token", required=True)
    p_update.add_argument("--project", required=True)
    p_update.add_argument("--variant", required=True)
    p_update.add_argument("--commit_id", required=True)

    p_notify = subparsers.add_parser("notify")
    p_notify.add_argument("--tag", required=True)

    args = parser.parse_args()

    if args.command == "parse":
        get_project_env(args.project)
    elif args.command == "meta":
        generate_build_meta(args.project, args.branch)
    elif args.command == "matrix":
        generate_release_matrix(args.project, args.token)
    elif args.command == "add":
        add_project(args)
    elif args.command == "setup":
        setup_repos(args)
    elif args.command == "watch":
        watch_upstream(args)
    elif args.command == "update":
        perform_update(args)
    elif args.command == "notify":
        send_telegram_notify(args)