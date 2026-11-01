# Kuberve Licensing FAQ

### Can I use Kuberve for free?
Yes, under AGPLv3 (or LGPLv3/MIT for specific components). The catch: if you
modify and distribute Kuberve, or offer it as a network service, you must
open your modifications.

### I'm building a closed-source app for Kuberve. Do I need a commercial license?
**No**, if you only use the Kuberve ABI (syscalls) and link against `libc`
(LGPLv3) or `abi` (MIT). The AGPLv3 copyleft does not extend to separate
programs that merely interact with the kernel via syscalls. We explicitly
grant a "System Call Exception" (see `LICENSE-EXCEPTION.md`).

### I'm forking Kuberve to make my own OS. Can I keep it closed?
**No.** If you distribute a modified kernel or userspace utilities, they must
remain AGPLv3. You can, however, sell hardware with that modified OS, as long
as you provide the source code and do not use the "Kuberve" trademark.

### Can I sell a device with Kuberve pre-installed?
Yes, if you comply with AGPLv3 (provide the source code) and do not use the
"Kuberve" name or logo without permission. If you want to call your device
"Kuberve", you need a commercial license and trademark agreement.

### Is SaaS use covered by AGPL?
Yes. If you run a modified version of Kuberve on a server and let users
interact with it over a network, AGPLv3 requires you to offer the source code
of your modifications to those users.

### I'm a student / hobbyist. Do I need to worry?
No. Personal use, learning, and non-distributed experiments are free. Just
don't redistribute modified code without following AGPLv3.