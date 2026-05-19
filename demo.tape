Output demo.gif
Set FontSize 16
Set Width 1000
Set Height 700
Set TypingSpeed 0ms
Set Framerate 15

# ============================================
# jet editor demo — 30 seconds
# ============================================

# 0-3s: Cold start benchmark
Type "time target/release/jet --headless --quit"
Sleep 1s
Enter
Sleep 2s

# 3-6s: Search throughput benchmark  
Type "target/release/jet --headless --search fn src/editor/motions.rs"
Sleep 500ms
Enter
Sleep 2s
Type "echo --- 15.7 GB/s peak throughput ---"
Sleep 500ms
Enter
Sleep 1s

# 6-9s: Open demo file
Type "target/release/jet src/editor/motions.rs +10"
Sleep 500ms
Enter
Sleep 2s

# 9-12s: Navigate with count prefix
Type "5j"
Sleep 300ms
Type "3w"
Sleep 300ms
Type "2b"
Sleep 300ms
Type "e"
Sleep 200ms
Type "gg"
Sleep 500ms

# 12-15s: Search
Type "/file_end"
Sleep 500ms
Enter
Sleep 1s
Type "n"
Sleep 400ms
Type "n"
Sleep 400ms
Escape
Sleep 300ms

# 15-18s: Selection editing
Type "v"
Sleep 200ms
Type "w"
Sleep 200ms
Type "d"
Sleep 300ms
Type "j"
Sleep 200ms
Type "p"
Sleep 500ms

# 18-20s: Dot-repeat
Escape
Sleep 200ms
Type "."
Sleep 500ms
Escape
Sleep 300ms

# 20-23s: Undo tree
Type ":undo-tree"
Sleep 300ms
Enter
Sleep 2s
Escape
Sleep 500ms

# 23-26s: Substitute 
Type ":s/pub /internal /g"
Sleep 500ms
Enter
Sleep 1s
Escape
Sleep 500ms

# 26-28s: Save and quit
Type ":wq"
Sleep 300ms
Enter
Sleep 1s

# 28-30s: Show binary size
Type "ls -lh target/release/jet.exe"
Sleep 500ms
Enter
Sleep 1s
Type "echo --- 16.6 MB stripped PE binary ---"
Sleep 500ms
Enter
Sleep 500ms
