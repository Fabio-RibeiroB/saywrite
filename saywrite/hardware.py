from __future__ import annotations

from dataclasses import dataclass
import os
from pathlib import Path
import shutil
import subprocess


@dataclass
class LocalRuntimeStatus:
    gpu_vendor: str
    acceleration: str
    whisper_cli_path: str | None
    cmake_available: bool
    vulkan_available: bool
    nvidia_smi_available: bool

    @property
    def runnable(self) -> bool:
        return self.whisper_cli_path is not None


def detect_local_runtime() -> LocalRuntimeStatus:
    gpu_vendor = detect_gpu_vendor()
    whisper_cli_path = detect_whisper_cli()
    cmake_available = shutil.which("cmake") is not None
    vulkan_available = _command_ok(["pkg-config", "--exists", "vulkan"])
    nvidia_smi_available = shutil.which("nvidia-smi") is not None

    if gpu_vendor == "nvidia" and nvidia_smi_available:
        acceleration = "cuda"
    elif gpu_vendor in {"amd", "intel"} and vulkan_available:
        acceleration = "vulkan"
    else:
        acceleration = "cpu"

    return LocalRuntimeStatus(
        gpu_vendor=gpu_vendor,
        acceleration=acceleration,
        whisper_cli_path=whisper_cli_path,
        cmake_available=cmake_available,
        vulkan_available=vulkan_available,
        nvidia_smi_available=nvidia_smi_available,
    )


def detect_gpu_vendor() -> str:
    if shutil.which("nvidia-smi") is not None:
        return "nvidia"

    output = _command_output(["lspci"])
    lowered = output.lower()
    if "amd" in lowered or "radeon" in lowered or "ati" in lowered:
        return "amd"
    if "intel" in lowered and any(token in lowered for token in ["vga", "3d controller", "display controller"]):
        return "intel"
    if "nvidia" in lowered:
        return "nvidia"
    return "unknown"


def detect_whisper_cli() -> str | None:
    override = os.environ.get("SAYWRITE_WHISPER_CLI")
    if override and _is_executable_file(Path(override)):
        return override

    repo_candidate = Path(__file__).resolve().parent.parent / "vendor" / "whisper.cpp" / "build" / "bin" / "whisper-cli"
    if _is_executable_file(repo_candidate):
        return str(repo_candidate)

    for candidate in ["whisper-cli", "main"]:
        path = shutil.which(candidate)
        if path:
            return path
    return None


def _is_executable_file(path: Path) -> bool:
    return path.exists() and path.is_file() and os.access(path, os.X_OK)


def _command_output(args: list[str]) -> str:
    try:
        result = subprocess.run(args, check=False, capture_output=True, text=True)
    except OSError:
        return ""
    return result.stdout


def _command_ok(args: list[str]) -> bool:
    try:
        result = subprocess.run(args, check=False, capture_output=True, text=True)
    except OSError:
        return False
    return result.returncode == 0
