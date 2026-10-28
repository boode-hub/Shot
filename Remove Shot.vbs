' Double-click to remove Shot. Asks first, then stops Shot, takes it out of start-up and,
' if you say so, moves this folder to the Recycle Bin. Runs Remove.ps1 next to this file.
Set fso = CreateObject("Scripting.FileSystemObject")
dir = fso.GetParentFolderName(WScript.ScriptFullName)
CreateObject("WScript.Shell").Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & dir & "\Remove.ps1""", 0, False
