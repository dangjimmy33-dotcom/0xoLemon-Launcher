APP_QSS = r"""
* {
    font-family: "Segoe UI Variable", "Segoe UI";
    font-size: 10pt;
}
QMainWindow, QWidget#AppRoot {
    background: rgba(7, 12, 18, 238);
    color: #f5f8fc;
}
QWidget#TopBar {
    background: rgba(10, 17, 25, 168);
    border-bottom: 1px solid rgba(116, 153, 194, 30);
}
QWidget { color: #f5f8fc; }

QLabel#AppTitle { font-size: 22pt; font-weight: 700; color: #fbfdff; }
QLabel#AppSubtitle { font-size: 9.5pt; color: #8da0b6; }
QLabel#PageTitle { font-size: 16pt; font-weight: 650; color: #f7faff; }
QLabel#SectionTitle { font-size: 11.5pt; font-weight: 650; color: #f1f6fc; }
QLabel#FieldLabel { font-size: 9.4pt; font-weight: 600; color: #cbd6e4; }
QLabel#Muted { color: #8394a8; font-size: 9pt; }
QLabel#Hint { color: #93a5bb; font-size: 9.2pt; }
QLabel#StatusGood { color: #8ce3b1; font-weight: 600; }
QLabel#StatusWarn { color: #ffd28a; font-weight: 600; }

QLabel#ChipBlue {
    background: rgba(44, 139, 255, 38);
    border: 1px solid rgba(73, 157, 255, 90);
    border-radius: 10px;
    padding: 6px 10px;
    color: #b9dcff;
    font-weight: 600;
}
QLabel#ChipGreen {
    background: rgba(45, 168, 105, 30);
    border: 1px solid rgba(78, 195, 136, 80);
    border-radius: 10px;
    padding: 6px 10px;
    color: #b8f0d0;
    font-weight: 600;
}
QLabel#ChipPurple {
    background: rgba(168, 85, 247, 35);
    border: 1px solid rgba(192, 132, 252, 90);
    border-radius: 10px;
    padding: 6px 10px;
    color: #e9d5ff;
    font-weight: 600;
}
QLabel#ChipNeutral {
    background: rgba(255,255,255,12);
    border: 1px solid rgba(255,255,255,26);
    border-radius: 10px;
    padding: 6px 10px;
    color: #b9c5d3;
    font-weight: 600;
}

QFrame#GlassCard {
    background: rgba(17, 27, 39, 205);
    border: 1px solid rgba(112, 145, 180, 38);
    border-radius: 18px;
}
QFrame#GlassCardStrong {
    background: rgba(19, 32, 47, 218);
    border: 1px solid rgba(81, 137, 198, 62);
    border-radius: 18px;
}
QFrame#InsetCard {
    background: rgba(7, 12, 18, 188);
    border: 1px solid rgba(110, 145, 180, 28);
    border-radius: 12px;
}
QFrame#Divider {
    background: rgba(122, 150, 181, 34);
    min-height: 1px;
    max-height: 1px;
    border: 0;
}

QLineEdit, QComboBox, QSpinBox, QDoubleSpinBox {
    min-height: 40px;
    padding: 0 12px;
    background: rgba(6, 11, 17, 205);
    border: 1px solid rgba(101, 133, 169, 58);
    border-radius: 9px;
    color: #f0f5fb;
    selection-background-color: #2d8cff;
}
QLineEdit:hover, QComboBox:hover, QSpinBox:hover, QDoubleSpinBox:hover { border-color: rgba(113, 159, 211, 100); background: rgba(8, 14, 21, 225); }
QLineEdit:focus, QComboBox:focus, QSpinBox:focus, QDoubleSpinBox:focus { border: 1px solid #438fdf; background: rgba(9, 16, 24, 240); }
QLineEdit:disabled, QComboBox:disabled, QSpinBox:disabled, QDoubleSpinBox:disabled { color: #637386; background: rgba(8,12,17,170); border-color: rgba(90,110,132,30); }
QComboBox::drop-down { border: 0; width: 30px; }
QComboBox QAbstractItemView {
    background: #121c27; color: #eef4fb; border: 1px solid #2c3a49;
    selection-background-color: #2d8cff; outline: 0; padding: 5px;
}

QPushButton {
    min-height: 38px;
    padding: 0 15px;
    border-radius: 9px;
    border: 1px solid rgba(104, 135, 168, 60);
    background: rgba(25, 37, 50, 225);
    color: #eef4fb;
    font-weight: 600;
}
QPushButton:hover { background: rgba(34, 50, 67, 235); border-color: rgba(120, 163, 209, 105); }
QPushButton:pressed { background: rgba(15, 24, 34, 235); }
QPushButton:disabled { background: rgba(15,22,30,190); border-color: rgba(90,110,132,28); color: #617184; }
QPushButton#Primary {
    min-height: 44px;
    min-width: 170px;
    background: #2d8cff;
    border: 1px solid #55a2ff;
    color: white;
    font-size: 10.5pt;
    font-weight: 700;
}
QPushButton#Primary:hover { background: #3994ff; }
QPushButton#Primary:pressed { background: #237ee8; }
QPushButton#Ghost { background: transparent; border-color: transparent; color: #9cc9fb; padding: 0 8px; }
QPushButton#Ghost:hover { background: rgba(45,140,255,20); border-color: rgba(80,150,225,42); }
QPushButton#Segment {
    min-height: 36px; padding: 0 15px; background: rgba(5,10,16,160);
    border: 1px solid rgba(100,130,160,52); border-radius: 8px; color: #9eafc3;
}
QPushButton#Segment:hover { background: rgba(17,28,40,210); color: #e0e8f1; }
QPushButton#Segment:checked { background: rgba(45,140,255,45); border-color: #3e94ef; color: #e4f1ff; }
QPushButton#ModeCard {
    min-height: 88px; text-align: left; padding: 14px 18px;
    background: rgba(14,22,31,210); border: 1px solid rgba(99,132,165,48); border-radius: 14px;
    color: #e9f0f8; font-size: 10.2pt; font-weight: 650;
}
QPushButton#ModeCard:hover { background: rgba(20,32,45,225); border-color: rgba(90,158,225,105); }
QPushButton#ModeCard:checked { background: rgba(37,109,188,62); border: 1px solid #3d91ee; color: #ffffff; }
QPushButton#ActivityHeader { min-height: 38px; padding: 0 2px; background: transparent; border: 0; text-align: left; color: #eaf0f7; font-weight: 650; }
QPushButton#ActivityHeader:hover { color: white; }

QCheckBox { spacing: 8px; color: #cbd6e3; }
QCheckBox::indicator { width: 17px; height: 17px; border-radius: 5px; border: 1px solid #425266; background: #0c131b; }
QCheckBox::indicator:checked { background: #2d8cff; border-color: #2d8cff; }

QProgressBar { min-height: 5px; max-height: 5px; background: rgba(33,48,64,180); border: 0; border-radius: 2px; color: transparent; }
QProgressBar::chunk { background: #2d8cff; border-radius: 2px; }
QPlainTextEdit {
    background: rgba(4,8,13,205); border: 1px solid rgba(94,126,159,40); border-radius: 11px;
    padding: 11px 13px; color: #bfcbd9; font-family: "Cascadia Mono", "Consolas"; font-size: 9pt;
    selection-background-color: #285e96;
}
QScrollArea { border: 0; background: transparent; }
QScrollArea > QWidget > QWidget { background: transparent; }
QScrollBar:vertical { background: transparent; width: 10px; margin: 3px; }
QScrollBar::handle:vertical { background: rgba(86,108,135,150); min-height: 30px; border-radius: 4px; }
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical { height: 0; }
QToolTip { background: #18232f; color: white; border: 1px solid #394b5f; padding: 6px; }
"""
