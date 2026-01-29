import os
import json
import argparse
import sys
import subprocess
import shutil
import re

CONFIG_PATH = "configs/projects.json"
TEMPLATES_DIR = "templates"
WORKSPACE_DIR = "kernel_workspace"

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
            raise e
        return None

def load_config():
    if not os.path.exists(CONFIG_PATH):
        return {}
    with open(CONFIG_PATH, 'r') as f:
        return json.load(f)

def save_config(data):
    with open(CONFIG_PATH, 'w') as f:
        json.dump(data, f, indent=2)
        f.write('\n')

def get_project_env(project_key):
    data = load_config()
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
    data = load_config()
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
    data = load_config()
    
    new_proj = {
        "repo": args.repo,
        "defconfig": args.defconfig,
        "localversion_base": args.localversion,
        "anykernel_repo": args.ak3_repo,
        "anykernel_branch": args.ak3_branch,
        "zip_name_prefix": args.zip_name,
        "supported_ksu": ["resukisu", "mksu", "ksu"],
        "push_server": {"enabled": False}
    }
    
    if args.toolchain_prefix:
        new_proj["toolchain_path_prefix"] = args.toolchain_prefix
        
    data[args.key] = new_proj
    save_config(data)

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
    data = load_config()
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
        if not repo_url:
            continue
            
        target_dir = os.path.join(WORKSPACE_DIR, key)
        auth_url = f"https://{token}@github.com/{repo_url}.git" if token else f"https://github.com/{repo_url}.git"
        
        if os.path.exists(target_dir):
            shutil.rmtree(target_dir)
        
        run_cmd(["git", "clone", auth_url, target_dir])
        
        readme_content = process_readme(readme_tpl, proj, repo_url, readme_lang)
        
        target_branches = ["main", "ksu", "mksu", "resukisu"]
        
        remote_branches_raw = run_cmd(["git", "branch", "-r"], cwd=target_dir)
        remote_branches = [b.strip().replace("origin/", "") for b in remote_branches_raw.splitlines() if "HEAD" not in b]
        
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
            
            if not os.path.exists(github_dir):
                os.makedirs(github_dir)
            
            src_funding = os.path.join(".github", "FUNDING.yml")
            if os.path.exists(src_funding):
                shutil.copy(src_funding, os.path.join(github_dir, "FUNDING.yml"))
            
            if os.path.exists(workflows_dir):
                shutil.rmtree(workflows_dir)
            os.makedirs(workflows_dir)
            
            for old_file in ["build.sh", "build_kernel.sh", "update.sh", "update-kernelsu.yml"]:
                old_path = os.path.join(cwd, old_file)
                if os.path.exists(old_path):
                    os.remove(old_path)
            
            trigger_content = trigger_tpl.replace("__PROJECT_KEY__", key)
            repo_owner = repo_url.split('/')[0] if '/' in repo_url else "YuzakiKokuban"
            trigger_content = trigger_content.replace("__REPO_OWNER__", repo_owner)
            
            with open(os.path.join(workflows_dir, "trigger-central-build.yml"), "w") as f:
                f.write(trigger_content)
                
            src_gitignore = os.path.join("configs", "universal.gitignore")
            if os.path.exists(src_gitignore):
                shutil.copy(src_gitignore, os.path.join(cwd, ".gitignore"))
                
            run_cmd(["git", "add", "."], cwd=cwd)
            status = run_cmd(["git", "status", "--porcelain"], cwd=cwd)
            if status:
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
                    
                    hook_config = [
                        "config[url]=" + webhook_url,
                        "config[content_type]=json",
                        "events[]=release",
                        "active=true"
                    ]
                    if webhook_secret:
                        hook_config.append(f"config[secret]={webhook_secret}")
                        
                    cmd_base = ["gh", "api", "--method"]
                    
                    if hooks_json:
                        hook_id = hooks_json.strip()
                        run_cmd(cmd_base + ["PATCH", f"repos/{repo_url}/hooks/{hook_id}"] + [f"-f{c}" for c in hook_config], check=False)
                    else:
                        run_cmd(cmd_base + ["POST", f"repos/{repo_url}/hooks", "-f", "name=web"] + [f"-f{c}" for c in hook_config], check=False)

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
    p_add.add_argument("--ak3_repo", default="https://github.com/YuzakiKokuban/AnyKernel3.git")
    p_add.add_argument("--ak3_branch", default="master")
    p_add.add_argument("--zip_name", default="Kernel")
    p_add.add_argument("--toolchain_prefix", default="")

    p_setup = subparsers.add_parser("setup")
    p_setup.add_argument("--token")
    p_setup.add_argument("--commit_message", default="[skip ci] ci: Sync central CI files")
    p_setup.add_argument("--readme_language", default="both", choices=["both", "zh-CN", "en-US"])

    args = parser.parse_args()
    
    if args.command == "parse":
        get_project_env(args.project)
    elif args.command == "matrix":
        generate_release_matrix(args.project, args.token)
    elif args.command == "add":
        add_project(args)
    elif args.command == "setup":
        setup_repos(args)