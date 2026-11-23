#!/usr/bin/env python3
"""Run train-whisper.py on one rented GPU (vast.ai) with a hard spending limit.

Steps, each logged to <workdir>/run.log:
  1. pick the cheapest offer that fits (verified datacenter, EU by default,
     one A100/H100 80 GB, enough disk and CPU) and rent it
  2. upload the data set (tar | zstd over ssh) and the training script
  3. start training in the background on the instance
  4. watch it every --every seconds: the estimated spend (price per hour x
     hours since start) must stay under --cap, the instance must stay up
  5. when training ends, download <out>/final, result.json and the log
  6. destroy the instance, always: on success, on the cap, on an error, on Ctrl-C

The API key never leaves this machine: the instance gets only the data and
the script. Its copy of the data goes with the instance when it is destroyed.
State (instance id, price, start time) is kept in <workdir>/state.json;
--destroy-only reads it and destroys a leftover instance.

Needs the vastai CLI with a stored key (vastai set api-key) and an ssh key
registered with vastai (vastai create ssh-key); --ssh-key names its private half.

Usage:
  scripts/vast-train.py <set-dir> <workdir> [--cap 30] [--gpus A100_SXM4,H100_SXM]
      [--geo eu|any] [--image pytorch/pytorch:2.15.0-cuda13.2-cudnn9-runtime]
      [--disk 200] [--every 300] [--ssh-key ~/.ssh/vast_ed25519] [--dry-run]
      [--train-args "--epochs 2 --batch 32 --accum 1"]
  scripts/vast-train.py <set-dir> <workdir> --destroy-only
"""

import argparse
import json
import shlex
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EU = "[AT,BE,BG,CZ,DE,DK,EE,ES,FI,FR,GR,HR,HU,IE,IT,LT,LU,LV,NL,NO,PL,PT,RO,SE,SI,SK,CH,IS]"
PIP = "torch==2.15.0 transformers==5.24.0 accelerate==1.16.0 soundfile==0.14.0 jiwer"


class Run:
    def __init__(self, a):
        self.a = a
        self.work = a.workdir.resolve()
        self.work.mkdir(parents=True, exist_ok=True)
        self.state_path = self.work / "state.json"
        self.state = json.loads(self.state_path.read_text()) if self.state_path.exists() else {}
        self.logf = open(self.work / "run.log", "a", encoding="utf-8")

    def log(self, msg):
        line = time.strftime("%Y-%m-%d %H:%M:%S ") + msg
        print(line, flush=True)
        self.logf.write(line + "\n")
        self.logf.flush()

    def save(self):
        self.state_path.write_text(json.dumps(self.state, indent=2) + "\n")

    def vast(self, *args, raw=True):
        cmd = ["vastai", *args] + (["--raw"] if raw else [])
        r = subprocess.run(cmd, capture_output=True, text=True)
        if r.returncode != 0:
            raise RuntimeError(f"vastai {' '.join(args[:2])}: {r.stderr.strip() or r.stdout.strip()}")
        return json.loads(r.stdout) if raw else r.stdout

    def ssh_base(self):
        return ["ssh", "-i", str(self.a.ssh_key), "-p", str(self.state["port"]), "-o", "StrictHostKeyChecking=accept-new",
                "-o", "ServerAliveInterval=30", "-o", f"UserKnownHostsFile={self.work / 'known_hosts'}",
                f"root@{self.state['host']}"]

    def ssh(self, command, check=True, **kw):
        return subprocess.run(self.ssh_base() + [command], check=check, **kw)

    def spent(self):
        if "start" not in self.state:
            return 0.0
        return self.state["dph"] * (time.time() - self.state["start"]) / 3600

    def destroy(self):
        iid = self.state.get("instance")
        if not iid:
            return
        for attempt in range(5):
            try:
                self.vast("destroy", "instance", str(iid), "-y", raw=False)
                time.sleep(10)
                if any(i.get("id") == iid for i in self.vast("show", "instances")):
                    raise RuntimeError("instance still listed after destroy")
                self.log(f"destroyed instance {iid} (checked: no longer listed); estimated spend {self.spent():.2f} USD")
                self.state["destroyed"] = True
                self.state.pop("instance")
                self.save()
                return
            except RuntimeError as e:
                self.log(f"destroy failed ({e}), retry {attempt + 1}")
                time.sleep(15)
        self.log(f"COULD NOT DESTROY instance {iid}: destroy it by hand (vastai destroy instance {iid})")

    def pick(self):
        geo = f" geolocation in {EU}" if self.a.geo == "eu" else ""
        q = (f"gpu_name in [{self.a.gpus}] num_gpus=1 gpu_ram>=79 datacenter=true verified=true reliability>0.98"
             f" cuda_max_good>=13.2 disk_space>={self.a.disk} cpu_cores_effective>=8 inet_down>=500 inet_up>=200 rentable=true{geo}")
        offers = sorted(self.vast("search", "offers", q, "-o", "dph"), key=lambda o: o["dph_total"])
        if not offers:
            raise RuntimeError("no offer matches: " + q)
        o = offers[0]
        self.log(f"offer {o['id']}: {o['gpu_name']} {round(o['gpu_ram'] / 1024)} GB, {o.get('geolocation')}, {o['dph_total']:.3f} USD/h")
        return o

    def rent(self):
        o = self.pick()
        if self.a.dry_run:
            self.log("dry run: not renting")
            return False
        r = self.vast("create", "instance", str(o["id"]), "--image", self.a.image, "--disk", str(self.a.disk),
                      "--ssh", "--direct", "--label", "pusmogo-ft", "--cancel-unavail")
        self.state.update({"instance": r["new_contract"], "dph": o["dph_total"], "start": time.time(), "offer": o["id"]})
        self.save()
        self.log(f"rented instance {self.state['instance']}")
        deadline = time.time() + 1200
        while time.time() < deadline:
            info = self.vast("show", "instance", str(self.state["instance"]))
            if info.get("actual_status") == "running":
                url = self.vast("ssh-url", str(self.state["instance"]), raw=False).strip()
                hostport = url.split("@", 1)[1]
                self.state["host"], self.state["port"] = hostport.rsplit(":", 1)
                self.state["dph"] = info.get("dph_total", self.state["dph"])
                self.save()
                for _ in range(20):
                    if self.ssh("true", check=False, capture_output=True).returncode == 0:
                        self.log(f"instance up at {self.state['host']}:{self.state['port']}")
                        return True
                    time.sleep(15)
                raise RuntimeError("instance runs but ssh does not answer")
            self.guard()
            time.sleep(20)
        raise RuntimeError("instance did not start within 20 min")

    def guard(self):
        s = self.spent()
        if s >= self.a.cap:
            raise RuntimeError(f"spending cap reached: estimated {s:.2f} USD >= {self.a.cap:.2f} USD")

    def upload(self):
        self.ssh("command -v zstd >/dev/null || (apt-get update -qq && apt-get install -y -qq zstd >/dev/null); mkdir -p /workspace")
        self.log("uploading the data set")
        tar = subprocess.Popen(["tar", "--no-xattrs", "-C", str(self.a.set.parent), "-cf", "-", self.a.set.name],
                               stdout=subprocess.PIPE, env={"COPYFILE_DISABLE": "1", "PATH": "/usr/bin:/bin"})
        z = subprocess.Popen(["zstd", "-1", "-T0", "-q"], stdin=tar.stdout, stdout=subprocess.PIPE)
        tar.stdout.close()
        self.ssh("zstd -d -q | tar -C /workspace -xf -", stdin=z.stdout)
        z.wait()
        if tar.wait() != 0 or z.returncode != 0:
            raise RuntimeError("upload failed")
        subprocess.run(["scp", "-i", str(self.a.ssh_key), "-P", str(self.state["port"]), "-o",
                        f"UserKnownHostsFile={self.work / 'known_hosts'}", str(ROOT / "scripts" / "train-whisper.py"),
                        f"root@{self.state['host']}:/workspace/"], check=True)
        self.log("upload done")

    def start(self):
        data = f"/workspace/{self.a.set.name}"
        cmd = ("cd /workspace && PY=/opt/conda/bin/python; [ -x $PY ] || PY=$(command -v python3); "
               f"$PY -m pip install -q --break-system-packages {PIP} > pip.log 2>&1 && "
               "$PY -c 'import torch; print(torch.__version__, torch.cuda.is_available())' >> pip.log 2>&1 && "
               f"$PY train-whisper.py {shlex.quote(data)} /workspace/out {self.a.train_args} > train.log 2>&1; "
               f"echo $? > /workspace/exit")
        self.ssh(f"nohup bash -c {shlex.quote(cmd)} > /dev/null 2>&1 &")
        self.log("training started: " + self.a.train_args)

    def watch(self):
        while True:
            self.guard()
            r = self.ssh("cat /workspace/exit 2>/dev/null; tail -c 30000 /workspace/train.log 2>/dev/null | tr '\\r' '\\n' | grep -E \"'loss'|eval_wer|Error|error\" | tail -2",
                         check=False, capture_output=True, text=True)
            if r.returncode != 0:
                info = self.vast("show", "instance", str(self.state["instance"]))
                if info.get("actual_status") != "running":
                    raise RuntimeError(f"instance is {info.get('actual_status')}")
                self.log("ssh did not answer; retrying")
            else:
                lines = r.stdout.strip().splitlines()
                if lines and lines[0].strip().isdigit():
                    code = int(lines[0])
                    self.log(f"training ended with exit code {code}")
                    return code
                self.log(f"spent ~{self.spent():.2f} USD | " + (lines[-1][:200] if lines else "starting"))
            time.sleep(self.a.every)

    def download(self):
        out = self.work / "out"
        out.mkdir(exist_ok=True)
        for remote in ["/workspace/train.log", "/workspace/pip.log", "/workspace/out/result.json", "/workspace/out/final"]:
            subprocess.run(["scp", "-r", "-i", str(self.a.ssh_key), "-P", str(self.state["port"]), "-o",
                            f"UserKnownHostsFile={self.work / 'known_hosts'}", f"root@{self.state['host']}:{remote}", str(out)],
                           check=False)
        self.log(f"downloaded results to {out}")


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("set", type=Path)
    p.add_argument("workdir", type=Path)
    p.add_argument("--cap", type=float, default=30.0)
    p.add_argument("--gpus", default="A100_SXM4,H100_SXM")
    p.add_argument("--geo", choices=["eu", "any"], default="eu")
    p.add_argument("--image", default="pytorch/pytorch:2.15.0-cuda13.2-cudnn9-runtime")
    p.add_argument("--disk", type=int, default=200)
    p.add_argument("--every", type=int, default=300)
    p.add_argument("--ssh-key", type=Path, default=Path.home() / ".ssh" / "vast_ed25519")
    p.add_argument("--train-args", default="--epochs 2 --batch 32 --accum 1")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--destroy-only", action="store_true")
    a = p.parse_args()
    a.set = a.set.resolve()

    run = Run(a)
    if a.destroy_only:
        run.destroy()
        return 0
    if run.state.get("instance"):
        run.log(f"state.json names instance {run.state['instance']}: run --destroy-only first")
        return 1
    if not (a.set / "train.jsonl").exists():
        p.error(f"{a.set} has no train.jsonl")
    code = 1
    try:
        if not run.rent():
            return 0
        run.upload()
        run.start()
        code = run.watch()
        run.download()
    except (Exception, KeyboardInterrupt) as e:
        run.log(f"stopping: {e}")
        try:
            if run.state.get("host"):
                run.download()
        except Exception:
            pass
    finally:
        run.destroy()
    return code


if __name__ == "__main__":
    sys.exit(main())
