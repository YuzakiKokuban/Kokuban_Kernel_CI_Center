import os
import json
import argparse
import sys
import subprocess
import shutil
import re
import urllib.request

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

def run_cmd(cmd, cwd=None, check=True):
    try:
        result = subprocess.run(
            cmd, 
            cwd=cwd, 
            check=check, 
            stdout=subprocess.PIPE, 
            stderr=subprocess.PIPE, 
            text=True
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError as e:
        if check:
            print(f"Command failed: {e.cmd}")
            print(f"Stderr: {e.stderr}")
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

def get_remote_head(repo_url, branch):
    cmd = ["git", "ls-remote", repo_url, branch]
    output = run_cmd(cmd)
    if output:
        return output.split()[0]
    return None

def get_project_env(project_key):
    data = load_json(CONFIG_PATH)
    if project_key not in data:
        sys.exit(1)
    
    proj = data[project_key]
    supported_ksu = [k.replace("sukisuultra", "resukisu") for k in proj.get("supported_ksu", [])]
    
    envs = {
        "PROJECT_REPO": proj.get("repo"),
        "PROJECT_DEFCONFIG": proj.get("defconfig"),
        "PROJECT_LOCALVERSION_BASE": proj.get("localversion_base"),
        "PROJECT_LTO": proj.get("lto", ""),
        "PROJECT_TOOLCHAIN_PREFIX": proj.get("toolchain_path_prefix", ""),
        "PROJECT_ZIP_NAME": proj.get("zip_name_prefix", "Kernel"),
        "PROJECT_AK3_REPO": proj.get("anykernel_repo"),
        "PROJECT_AK3_BRANCH": proj.get("anykernel_branch"),
        "PROJECT_VERSION_METHOD": proj.get("version_method", "param"),
        "PROJECT_EXTRA_HOST_ENV": str(proj.get("extra_host_env", False)).lower(),
    }
    
    envs["PROJECT_TOOLCHAIN_EXPORTS"] = json.dumps(proj.get("toolchain_path_exports", []))
    envs["PROJECT_DISABLE_SECURITY"] = json.dumps(proj.get("disable_security", []))
    
    if "toolchain_urls" in proj:
        envs["PROJECT_TOOLCHAIN_URLS"] = json.dumps(proj["toolchain_urls"])
    
    if "GITHUB_ENV" in os.environ:
        with open(os.environ["GITHUB_ENV"], "a") as f:
            for k, v in envs.items():
                f.write(f"{k}={v}\n")
    else:
        for k, v in envs.items():
            print(f"{k}={v}")

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
        },
        "push_server": {"enabled": False}
    }
    
    if args.toolchain_prefix:
        new_proj["toolchain_path_prefix"] = args.toolchain_prefix
        
    data[args.key] = new_proj
    save_json(CONFIG_PATH, data)

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
        repo_url = proj.get("repo")
        if not repo_url: continue
            
        target_dir = os.path.join(WORKSPACE_DIR, key)
        auth_url = f"https://{token}@github.com/{repo_url}.git" if token else f"https://github.com/{repo_url}.git"
        
        if os.path.exists(target_dir):
            shutil.rmtree(target_dir)
        run_cmd(["git", "clone", auth_url, target_dir])
        
        readme_content = process_readme(readme_tpl, proj, repo_url, readme_lang)
        target_branches = ["main", "ksu", "mksu", "resukisu"]
        remote_branches = [b.strip().replace("origin/", "") for b in run_cmd(["git", "branch", "-r"], cwd=target_dir).splitlines()]
        
        for branch in target_branches:
            cwd = target_dir
            branch_exists = branch in remote_branches
            
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
            if run_cmd(["git", "status", "--porcelain"], cwd=cwd):
                run_cmd(["git", "commit", "-m", f"{commit_msg} (branch: {branch})"], cwd=cwd)
                run_cmd(["git", "push", "origin", branch], cwd=cwd)

        if args.token:
            run_cmd(["gh", "api", "--method", "PATCH", f"repos/{repo_url}", "-f", "has_sponsorships=true", "--silent"], check=False)
            push_server = proj.get("push_server", {})
            if push_server.get("enabled"):
                webhook_url = push_server.get("webhook_url")
                webhook_secret = os.environ.get("WEBHOOK_SECRET", "")
                if webhook_url:
                    hooks_json = run_cmd(["gh", "api", f"repos/{repo_url}/hooks", "--jq", f".[] | select(.config.url == \"{webhook_url}\") | .id"], check=False)
                    hook_config = ["config[url]="+webhook_url, "config[content_type]=json", "events[]=release", "active=true"]
                    if webhook_secret: hook_config.append(f"config[secret]={webhook_secret}")
                    
                    if hooks_json:
                        run_cmd(["gh", "api", "--method", "PATCH", f"repos/{repo_url}/hooks/{hooks_json.strip()}"] + [f"-f{c}" for c in hook_config], check=False)
                    else:
                        run_cmd(["gh", "api", "--method", "POST", f"repos/{repo_url}/hooks", "-f", "name=web"] + [f"-f{c}" for c in hook_config], check=False)

def watch_upstream(args):
    track_data = load_json(UPSTREAM_PATH)
    projects_data = load_json(CONFIG_PATH)
    update_matrix = []
    
    track_data.pop("sukisuultra", None)
    
    for variant, config in KSU_CONFIG.items():
        print(f"Checking upstream for {variant}...")
        latest_hash = get_remote_head(config["repo"], config["branch"])
        
        if not latest_hash:
            print(f"Failed to get remote head for {variant}")
            continue
            
        stored_hash = track_data.get(variant, "")
        
        if latest_hash != stored_hash:
            print(f"New update found for {variant}: {stored_hash} -> {latest_hash}")
            track_data[variant] = latest_hash
            
            for p_key, p_data in projects_data.items():
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
        print(f"Project {project_key} not found")
        sys.exit(1)
        
    repo_url = data[project_key]["repo"]
    target_dir = "temp_kernel"
    
    if os.path.exists(target_dir):
        shutil.rmtree(target_dir)
        
    auth_url = f"https://{token}@github.com/{repo_url}.git"
    print(f"Cloning {project_key} ({variant})...")
    
    run_cmd(["git", "clone", "--depth=1", "--branch", variant, auth_url, target_dir])
    
    with open(os.path.join(target_dir, "KERNELSU_VERSION.txt"), "w") as f:
        f.write(commit_id)
        
    src_gitignore = os.path.join("configs", "universal.gitignore")
    if os.path.exists(src_gitignore):
        shutil.copy(src_gitignore, os.path.join(target_dir, ".gitignore"))
        
    ksu_cfg = KSU_CONFIG.get(variant)
    if ksu_cfg:
        setup_script_path = os.path.join(target_dir, "setup.sh")
        print(f"Downloading setup script from {ksu_cfg['setup_url']}")
        
        with urllib.request.urlopen(ksu_cfg['setup_url']) as response, open(setup_script_path, 'wb') as out_file:
            shutil.copyfileobj(response, out_file)
            
        print("Running setup script...")
        cmd = ["bash", "setup.sh"] + ksu_cfg["setup_args"]
        run_cmd(cmd, cwd=target_dir)
        os.remove(setup_script_path)
        
    run_cmd(["git", "config", "user.name", "Kokuban-Bot"], cwd=target_dir)
    run_cmd(["git", "config", "user.email", "bot@kokuban.dev"], cwd=target_dir)
    
    run_cmd(["git", "add", "."], cwd=target_dir)
    if run_cmd(["git", "status", "--porcelain"], cwd=target_dir):
        msg = f"ci: update {variant} to {commit_id}"
        run_cmd(["git", "commit", "-m", msg], cwd=target_dir)
        run_cmd(["git", "push"], cwd=target_dir)
        print(f"Pushed update for {project_key}")
    else:
        print("No changes to push")
        
    shutil.rmtree(target_dir)

if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command")
    
    p_parse = subparsers.add_parser("parse")
    p_parse.add_argument("--project", required=True)
    
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

    args = parser.parse_args()
    
    if args.command == "parse":
        get_project_env(args.project)
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