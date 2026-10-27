' Double-click to remove Shot. Asks first, then stops Shot, takes it out of start-up and,
' if you say so, moves this folder to the Recycle Bin. Runs Shot.ps1 -Remove next to this file.
Set fso = CreateObject("Scripting.FileSystemObject")
dir = fso.GetParentFolderName(WScript.ScriptFullName)
CreateObject("WScript.Shell").Run "powershell.exe -NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File """ & dir & "\Shot.ps1"" -Remove", 0, False
