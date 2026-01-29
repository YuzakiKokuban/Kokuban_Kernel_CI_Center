import os
import json
import argparse
import sys
import subprocess

CONFIG_PATH = "configs/projects.json"

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
        print(f"Error: Project {project_key} not found.")
        sys.exit(1)
    
    proj = data[project_key]
    
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
    
    supported = data[project_key].get("supported_ksu", [])
    branches = ["main"] + supported
    
    matrix = {"include": [{"branch": b} for b in branches]}
    
    if "GITHUB_OUTPUT" in os.environ:
        with open(os.environ["GITHUB_OUTPUT"], "a") as f:
            f.write(f"matrix={json.dumps(matrix)}\n")
    else:
        print(json.dumps(matrix))

def add_project(args):
    data = load_config()
    if args.key in data:
        print(f"Warning: Overwriting existing project {args.key}")
    
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
    print(f"Project {args.key} added successfully.")

def setup_repos(token):
    data = load_config()
    base_dir = "kernel_workspace"
    if not os.path.exists(base_dir):
        os.makedirs(base_dir)
    
    for key, proj in data.items():
        repo_url = proj.get("repo")
        if not repo_url:
            continue
            
        target_dir = os.path.join(base_dir, key)
        full_repo_url = f"https://{token}@github.com/{repo_url}.git" if token else f"https://github.com/{repo_url}.git"
        
        print(f"Processing {key}...")
        
        if os.path.exists(target_dir):
            print(f"  {key} exists, pulling...")
            subprocess.run(["git", "-C", target_dir, "pull"], check=False)
        else:
            print(f"  Cloning {key}...")
            subprocess.run(["git", "clone", full_repo_url, target_dir], check=True)

        subprocess.run(["git", "-C", target_dir, "config", "user.name", "Kokuban-CI"], check=False)
        subprocess.run(["git", "-C", target_dir, "config", "user.email", "ci@kokuban.local"], check=False)

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

    args = parser.parse_args()
    
    if args.command == "parse":
        get_project_env(args.project)
    elif args.command == "matrix":
        generate_release_matrix(args.project, args.token)
    elif args.command == "add":
        add_project(args)
    elif args.command == "setup":
        setup_repos(args.token)