---
fangst-downloads: minor
type: feat
---

Downloaded RAR and 7z archives are extracted, including split and password-protected ones, with the password from the NZB or from a job name ending in {{password}}. A safety check runs first and fails the job when it holds an executable or an archive entry that would land outside the job folder. Turn both on with `--enable postproc-safety --enable postproc-extract`.
