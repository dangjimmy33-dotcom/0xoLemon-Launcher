"""Regression tests for the frozen GUI entrypoint without importing Qt."""

import ast
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_gui_entrypoint_exists_in_main_window_source():
    tree = ast.parse((ROOT / "gse_autosetup" / "ui" / "main_window.py").read_text(encoding="utf-8"))
    names = {node.name for node in tree.body if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))}
    assert "run_app" in names


def test_app_py_imports_run_app_from_main_window():
    tree = ast.parse((ROOT / "app.py").read_text(encoding="utf-8"))
    imports = [node for node in tree.body if isinstance(node, ast.ImportFrom)]
    assert any(
        node.module == "gse_autosetup.ui.main_window"
        and any(alias.name == "run_app" for alias in node.names)
        for node in imports
    )
