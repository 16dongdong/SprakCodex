; 旧观测 DLL 已由 EXE 内嵌载荷替代。若历史客户端仍映射该文件，NSIS 在下次重启完成删除。
!macro NSIS_HOOK_PREINSTALL
  Delete /REBOOTOK "$INSTDIR\observationHook9.dll"
!macroend
