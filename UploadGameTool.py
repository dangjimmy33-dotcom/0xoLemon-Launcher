import tkinter as tk
from tkinter import filedialog, messagebox, ttk
import subprocess
import threading
import os

BASE_DIR = os.path.dirname(os.path.abspath(__file__))

PROJECTS = {
    "0xoLemon (xolemon-b360e)": {
        "detail":  "upload_single_detail.mjs",
        "catalog": "upload_single_catalog.mjs",
        "color":   "#e6a817",   # vàng
        "badge":   "🟡",
    },
    "0xoLemon-1 (xolemon-1)": {
        "detail":  "upload_single_detail_1.mjs",
        "catalog": "upload_single_catalog_1.mjs",
        "color":   "#4a9eff",   # xanh
        "badge":   "🔵",
    },
}

class App:
    def __init__(self, root):
        self.root = root
        self.root.title("0xoLemon Game Uploader")
        self.root.geometry("700x560")
        self.root.configure(bg="#1a1a2e")

        # ── Header ──────────────────────────────────────────────
        header = tk.Frame(root, bg="#16213e", pady=8)
        header.pack(fill="x")
        tk.Label(header, text="0xoLemon Game Uploader", font=("Consolas", 13, "bold"),
                 bg="#16213e", fg="#e6a817").pack()

        # ── Project selector ────────────────────────────────────
        proj_frame = tk.Frame(root, bg="#1a1a2e", pady=8)
        proj_frame.pack(fill="x", padx=16)

        tk.Label(proj_frame, text="Firebase Project:", font=("Arial", 9, "bold"),
                 bg="#1a1a2e", fg="#aaa").pack(side="left", padx=(0, 10))

        self.project_var = tk.StringVar(value=list(PROJECTS.keys())[0])
        for name in PROJECTS:
            badge = PROJECTS[name]["badge"]
            color = PROJECTS[name]["color"]
            rb = tk.Radiobutton(
                proj_frame, text=f"{badge} {name}",
                variable=self.project_var, value=name,
                font=("Arial", 9), bg="#1a1a2e", fg=color,
                selectcolor="#1a1a2e", activebackground="#1a1a2e",
                command=self.on_project_change
            )
            rb.pack(side="left", padx=8)

        # ── Inputs ──────────────────────────────────────────────
        frame_input = tk.Frame(root, pady=8, padx=16, bg="#1a1a2e")
        frame_input.pack(fill="x")

        tk.Label(frame_input, text="Game ID (Firebase):", width=22, anchor="w",
                 bg="#1a1a2e", fg="#ccc", font=("Arial", 9)).grid(row=0, column=0, pady=5)
        self.entry_game_id = tk.Entry(frame_input, width=38, bg="#0f3460", fg="white",
                                      insertbackground="white", font=("Consolas", 9))
        self.entry_game_id.grid(row=0, column=1, pady=5, padx=(0, 8))

        tk.Label(frame_input, text="Thư mục (src/assets):", width=22, anchor="w",
                 bg="#1a1a2e", fg="#ccc", font=("Arial", 9)).grid(row=1, column=0, pady=5)
        self.entry_folder = tk.Entry(frame_input, width=38, bg="#0f3460", fg="white",
                                     insertbackground="white", font=("Consolas", 9))
        self.entry_folder.grid(row=1, column=1, pady=5, padx=(0, 8))
        tk.Button(frame_input, text="📂 Chọn...", command=self.browse_folder,
                  bg="#0f3460", fg="white", font=("Arial", 8), relief="flat",
                  padx=6).grid(row=1, column=2, padx=4)

        # ── Buttons ─────────────────────────────────────────────
        self.frame_btn = tk.Frame(root, pady=10, bg="#1a1a2e")
        self.frame_btn.pack(fill="x", padx=16)
        self._build_buttons()

        # ── Log ─────────────────────────────────────────────────
        log_label = tk.Frame(root, bg="#1a1a2e")
        log_label.pack(fill="x", padx=16)
        tk.Label(log_label, text="📋 Logs:", font=("Arial", 9, "bold"),
                 bg="#1a1a2e", fg="#aaa").pack(anchor="w")

        self.text_log = tk.Text(root, height=16, state="disabled",
                                bg="#0d0d0d", fg="#00ff00",
                                font=("Consolas", 9), relief="flat",
                                padx=8, pady=6)
        self.text_log.pack(fill="both", expand=True, padx=16, pady=(0, 10))

        self.log("Sẵn sàng. Chọn Firebase project và thư mục game cần upload!")

    def _build_buttons(self):
        for w in self.frame_btn.winfo_children():
            w.destroy()

        proj = PROJECTS[self.project_var.get()]
        c = proj["color"]
        badge = proj["badge"]

        tk.Button(self.frame_btn,
                  text=f"{badge} 1. Up Metadata (Detail)",
                  width=24, bg="#0f3460", fg=c, font=("Arial", 9), relief="flat",
                  command=lambda: self.run_script(proj["detail"])
                  ).pack(side="left", padx=(0, 8))

        tk.Button(self.frame_btn,
                  text=f"{badge} 2. Up Catalog",
                  width=24, bg="#0f3460", fg=c, font=("Arial", 9), relief="flat",
                  command=lambda: self.run_script(proj["catalog"])
                  ).pack(side="left", padx=(0, 8))

        tk.Button(self.frame_btn,
                  text=f"{badge} Up Cả Hai",
                  width=16, bg=c, fg="#000", font=("Arial", 9, "bold"), relief="flat",
                  command=self.run_both
                  ).pack(side="left", padx=(0, 0))

    def on_project_change(self):
        self._build_buttons()
        proj = PROJECTS[self.project_var.get()]
        self.log(f"\n>>> Đã chọn project: {self.project_var.get()}")
        self.log(f"    Detail:  {proj['detail']}")
        self.log(f"    Catalog: {proj['catalog']}")

    def browse_folder(self):
        assets_dir = os.path.join(BASE_DIR, "src", "assets")
        initial = assets_dir if os.path.exists(assets_dir) else BASE_DIR
        path = filedialog.askdirectory(initialdir=initial,
                                       title="Chọn thư mục game (bên trong src/assets)")
        if path:
            folder_name = os.path.basename(path)
            self.entry_folder.delete(0, tk.END)
            self.entry_folder.insert(0, folder_name)
            if not self.entry_game_id.get():
                guessed_id = folder_name.lower().replace(" ", "-")
                self.entry_game_id.insert(0, guessed_id)

    def log(self, msg):
        self.text_log.config(state="normal")
        self.text_log.insert(tk.END, msg + "\n")
        self.text_log.see(tk.END)
        self.text_log.config(state="disabled")

    def run_command_thread(self, script_name, game_id, folder):
        proj_name = self.project_var.get()
        self.root.after(0, self.log, f"\n--- [{proj_name}] Chạy: {script_name} ---")
        try:
            script_path = os.path.join(BASE_DIR, script_name)
            if not os.path.isfile(script_path):
                self.root.after(0, self.log, f"Lỗi: Không tìm thấy script: {script_path}")
                return False
            process = subprocess.Popen(
                ["node", script_path, game_id, folder],
                cwd=BASE_DIR,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                encoding='utf-8',
                creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0
            )
            for line in iter(process.stdout.readline, ''):
                if line:
                    self.root.after(0, self.log, line.strip())
            process.wait()
            if process.returncode != 0:
                self.root.after(0, self.log, f"--- THẤT BẠI ({process.returncode}): {script_name} ---")
                return False
            self.root.after(0, self.log, f"--- Hoàn tất: {script_name} ---")
            return True
        except Exception as e:
            self.root.after(0, self.log, f"Lỗi không thể chạy: {str(e)}")
            return False

    def run_script(self, script_name):
        game_id = self.entry_game_id.get().strip()
        folder  = self.entry_folder.get().strip()
        if not game_id or not folder:
            messagebox.showwarning("Cảnh báo",
                "Vui lòng nhập đủ Game ID và Tên thư mục!")
            return
        threading.Thread(target=self.run_command_thread,
                         args=(script_name, game_id, folder), daemon=True).start()

    def run_both(self):
        game_id = self.entry_game_id.get().strip()
        folder  = self.entry_folder.get().strip()
        if not game_id or not folder:
            messagebox.showwarning("Cảnh báo",
                "Vui lòng nhập đủ Game ID và Tên thư mục!")
            return

        proj = PROJECTS[self.project_var.get()]

        def task():
            detail_ok = self.run_command_thread(proj["detail"], game_id, folder)
            if not detail_ok:
                self.root.after(0, lambda: messagebox.showerror(
                    "Upload thất bại",
                    f"Detail upload thất bại cho [{game_id}]. Catalog chưa được chạy để tránh publish trạng thái dở dang."
                ))
                return
            catalog_ok = self.run_command_thread(proj["catalog"], game_id, folder)
            if not catalog_ok:
                self.root.after(0, lambda: messagebox.showerror(
                    "Upload thất bại",
                    f"Catalog upload thất bại cho [{game_id}]. Kiểm tra log trước khi refresh cache."
                ))
                return
            self.root.after(0, lambda: messagebox.showinfo(
                "Thành công",
                f"✅ Upload xong Detail + Catalog cho [{game_id}]\n"
                f"Project: {self.project_var.get()}"
            ))

        threading.Thread(target=task, daemon=True).start()


if __name__ == "__main__":
    root = tk.Tk()
    app = App(root)
    root.mainloop()
