import tkinter as tk
from tkinter import filedialog, messagebox
import subprocess
import threading
import os

class App:
    def __init__(self, root):
        self.root = root
        self.root.title("0xoLemon Game Uploader")
        self.root.geometry("650x450")
        
        # Frame cho input
        frame_input = tk.Frame(root, pady=10, padx=10)
        frame_input.pack(fill="x")
        
        tk.Label(frame_input, text="Game ID (Firebase):", width=20, anchor="w").grid(row=0, column=0, pady=5)
        self.entry_game_id = tk.Entry(frame_input, width=40)
        self.entry_game_id.grid(row=0, column=1, pady=5)
        
        tk.Label(frame_input, text="Tên thư mục (src/assets):", width=20, anchor="w").grid(row=1, column=0, pady=5)
        self.entry_folder = tk.Entry(frame_input, width=40)
        self.entry_folder.grid(row=1, column=1, pady=5)
        
        tk.Button(frame_input, text="Chọn Thư Mục...", command=self.browse_folder).grid(row=1, column=2, padx=10, pady=5)
        
        # Frame cho button
        frame_btn = tk.Frame(root, pady=10)
        frame_btn.pack(fill="x")
        
        tk.Button(frame_btn, text="1. Up Metadata (Detail)", width=22, command=lambda: self.run_script("upload_single_detail.mjs")).pack(side="left", padx=10)
        tk.Button(frame_btn, text="2. Up Catalog", width=22, command=lambda: self.run_script("upload_single_catalog.mjs")).pack(side="left", padx=10)
        tk.Button(frame_btn, text="Up Cả Hai (Detail + Catalog)", width=25, bg="#4CAF50", fg="white", font=("Arial", 9, "bold"), command=self.run_both).pack(side="left", padx=10)
        
        # Log
        tk.Label(root, text="Logs output:", font=("Arial", 9, "bold")).pack(anchor="w", padx=10)
        self.text_log = tk.Text(root, height=15, state="disabled", bg="#1e1e1e", fg="#00ff00", font=("Consolas", 9))
        self.text_log.pack(fill="both", expand=True, padx=10, pady=5)
        
        self.log("Sẵn sàng. Hãy chọn thư mục game cần upload!")
        
    def browse_folder(self):
        assets_dir = os.path.join(os.getcwd(), "src", "assets")
        initial = assets_dir if os.path.exists(assets_dir) else os.getcwd()
        path = filedialog.askdirectory(initialdir=initial, title="Chọn thư mục game (bên trong src/assets)")
        if path:
            folder_name = os.path.basename(path)
            self.entry_folder.delete(0, tk.END)
            self.entry_folder.insert(0, folder_name)
            
            # Tự động đoán game ID nếu trống (viết thường, thay khoảng trắng thành gạch ngang)
            if not self.entry_game_id.get():
                guessed_id = folder_name.lower().replace(" ", "-")
                self.entry_game_id.insert(0, guessed_id)

    def log(self, msg):
        self.text_log.config(state="normal")
        self.text_log.insert(tk.END, msg + "\n")
        self.text_log.see(tk.END)
        self.text_log.config(state="disabled")
        
    def run_command_thread(self, script_name, game_id, folder):
        self.root.after(0, self.log, f"\n--- Đang chạy: {script_name} ---")
        try:
            # Dùng subprocess để chạy script node và bắt stdout/stderr
            process = subprocess.Popen(["node", script_name, game_id, folder],
                                       stdout=subprocess.PIPE,
                                       stderr=subprocess.STDOUT,
                                       text=True,
                                       encoding='utf-8',
                                       creationflags=subprocess.CREATE_NO_WINDOW if os.name == 'nt' else 0)
            
            for line in iter(process.stdout.readline, ''):
                if line:
                    self.root.after(0, self.log, line.strip())
                    
            process.wait()
            self.root.after(0, self.log, f"--- Hoàn tất: {script_name} ---")
        except Exception as e:
            self.root.after(0, self.log, f"Lỗi không thể chạy: {str(e)}")

    def run_script(self, script_name):
        game_id = self.entry_game_id.get().strip()
        folder = self.entry_folder.get().strip()
        if not game_id or not folder:
            messagebox.showwarning("Cảnh báo", "Vui lòng nhập đủ Game ID và Tên thư mục (hoặc nhấn Chọn Thư Mục)!")
            return
        threading.Thread(target=self.run_command_thread, args=(script_name, game_id, folder), daemon=True).start()

    def run_both(self):
        game_id = self.entry_game_id.get().strip()
        folder = self.entry_folder.get().strip()
        if not game_id or not folder:
            messagebox.showwarning("Cảnh báo", "Vui lòng nhập đủ Game ID và Tên thư mục (hoặc nhấn Chọn Thư Mục)!")
            return
            
        def task():
            self.run_command_thread("upload_single_detail.mjs", game_id, folder)
            self.run_command_thread("upload_single_catalog.mjs", game_id, folder)
            self.root.after(0, lambda: messagebox.showinfo("Thành công", f"Đã upload xong cả Metadata và Catalog cho {game_id}!"))
            
        threading.Thread(target=task, daemon=True).start()

if __name__ == "__main__":
    root = tk.Tk()
    app = App(root)
    root.mainloop()
