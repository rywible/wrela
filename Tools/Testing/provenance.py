"""Canonical, reproducible inputs for built Wrela products and harness runs."""
import hashlib
from pathlib import Path


BUILD_ROOTS = ("Engine", "Games", "Tools")
HARNESS_ROOTS = BUILD_ROOTS + ("scripts", "Testing")
SOURCE_SUFFIXES = frozenset({
    ".c", ".cpp", ".h", ".hpp", ".json", ".m", ".metal", ".mm", ".plist", ".py", ".swift",
})
IGNORED_PARTS = frozenset({".build", ".git", "Baselines", "__pycache__"})


def _included(path: Path, root: Path, *, script: bool) -> bool:
    relative = path.relative_to(root)
    if any(part in IGNORED_PARTS for part in relative.parts) or path.is_symlink() or not path.is_file():
        return False
    return path.suffix.lower() in SOURCE_SUFFIXES or (script and not path.suffix)


def input_paths(root: Path, roots: tuple[str, ...], *, include_scripts: bool = False) -> list[Path]:
    """Return only real source/authored inputs, in the order used for hashing."""
    result = [root / "Package.swift"]
    for name in roots:
        folder = root / name
        if not folder.is_dir() or folder.is_symlink():
            raise RuntimeError(f"Missing or linked candidate input root: {name}")
        result.extend(
            path for path in folder.rglob("*")
            if _included(path, root, script=include_scripts and name in {"scripts", "Testing"})
        )
    if not result[0].is_file() or result[0].is_symlink():
        raise RuntimeError("Missing or linked candidate input: Package.swift")
    return sorted(result, key=lambda path: path.relative_to(root).as_posix())


def digest_paths(root: Path, paths: list[Path]) -> str:
    digest = hashlib.sha256()
    for path in paths:
        digest.update(path.relative_to(root).as_posix().encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def build_source_digest(root: Path) -> str:
    """The digest embedded by scripts/build in every app bundle."""
    return digest_paths(root, input_paths(root, BUILD_ROOTS))


def harness_source_digest(root: Path) -> str:
    """Implementation, tests and validation scripts recorded in harness artifacts."""
    return digest_paths(root, input_paths(root, HARNESS_ROOTS, include_scripts=True))
