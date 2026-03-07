!macro NSIS_HOOK_POSTINSTALL
    MessageBox MB_YESNO "Thank you for installing StillSound! $\r$\n$\r$\nWould you like to open our GitHub page? Star the repo if you find it useful!" IDYES openGithub IDNO skipGithub
    openGithub:
        ExecShell "open" "https://github.com/saketjndl/StillSound-Studio"
    skipGithub:
!macroend
