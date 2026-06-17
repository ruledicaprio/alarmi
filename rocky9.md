[root@rocky9 /]# ls
afs  boot  etc   lib    lost+found  mnt  proc  run   selinux  sys  usr
bin  dev   home  lib64  media       opt  root  sbin  srv      tmp  var
[root@rocky9 /]# ls -la
total 76
dr-xr-xr-x  20 root   root    4096 Apr 17 07:10 .
dr-xr-xr-x  20 root   root    4096 Apr 17 07:10 ..
dr-xr-xr-x   2 root   root    4096 Nov  3  2024 afs
-rw-r--r--   1 root   root       0 Apr 17 06:59 .autorelabel
lrwxrwxrwx   1 root   root       7 Nov  3  2024 bin -> usr/bin
dr-xr-xr-x   2 root   root    4096 Nov  3  2024 boot
drwxr-xr-x   6 root   root     480 Nov  3  2024 dev
drwxr-xr-x  69 root   root    4096 Apr 17 07:18 etc
drwxr-xr-x   2 root   root    4096 Nov  3  2024 home
lrwxrwxrwx   1 root   root       7 Nov  3  2024 lib -> usr/lib
lrwxrwxrwx   1 root   root       9 Nov  3  2024 lib64 -> usr/lib64
drwx------   2 nobody nobody 16384 Apr 17 06:58 lost+found
drwxr-xr-x   2 root   root    4096 Nov  3  2024 media
drwxr-xr-x   2 root   root    4096 Nov  3  2024 mnt
drwxr-xr-x   3 root   root    4096 May  6 02:34 opt
dr-xr-xr-x 508 nobody nobody     0 Apr 17 06:59 proc
dr-xr-x---   6 root   root    4096 May  8 09:17 root
drwxr-xr-x  21 root   root     580 Apr 17 07:18 run
lrwxrwxrwx   1 root   root       8 Nov  3  2024 sbin -> usr/sbin
drwxr-xr-x   2 root   root    4096 Sep 12  2024 selinux
drwxr-xr-x   2 root   root    4096 Nov  3  2024 srv
dr-xr-xr-x  13 nobody nobody     0 Apr 17 06:59 sys
drwxrwxrwt  17 root   root    4096 May 13 20:42 tmp
drwxr-xr-x  12 root   root    4096 Apr 17 07:10 usr
drwxr-xr-x  18 root   root    4096 Apr 17 07:10 var
[root@rocky9 /]# cat /etc/os-release
NAME="Rocky Linux"
VERSION="9.7 (Blue Onyx)"
ID="rocky"
ID_LIKE="rhel centos fedora"
VERSION_ID="9.7"
PLATFORM_ID="platform:el9"
PRETTY_NAME="Rocky Linux 9.7 (Blue Onyx)"
ANSI_COLOR="0;32"
LOGO="fedora-logo-icon"
CPE_NAME="cpe:/o:rocky:rocky:9::baseos"
HOME_URL="https://rockylinux.org/"
VENDOR_NAME="RESF"
VENDOR_URL="https://resf.org/"
BUG_REPORT_URL="https://bugs.rockylinux.org/"
SUPPORT_END="2032-05-31"
ROCKY_SUPPORT_PRODUCT="Rocky-Linux-9"
ROCKY_SUPPORT_PRODUCT_VERSION="9.7"
REDHAT_SUPPORT_PRODUCT="Rocky Linux"
REDHAT_SUPPORT_PRODUCT_VERSION="9.7"
[root@rocky9 /]# uname -a
Linux rocky9 6.14.8-2-pve #1 SMP PREEMPT_DYNAMIC PMX 6.14.8-2 (2025-07-22T10:04Z) x86_64 x86_64 x86_64 GNU/Linux
[root@rocky9 /]# uptime -p
up 3 weeks, 5 days, 16 hours, 5 minutes
[root@rocky9 /]# hostnamectl
 Static hostname: rocky9
       Icon name: computer-container
         Chassis: container ☐
      Machine ID: 73e350b33e1542b695ebd1df1e173bfa
         Boot ID: 87ba7f01abc14ddc83df971c0056de86
  Virtualization: lxc
Operating System: Rocky Linux 9.7 (Blue Onyx)     
     CPE OS Name: cpe:/o:rocky:rocky:9::baseos
          Kernel: Linux 6.14.8-2-pve
    Architecture: x86-64
Firmware Version: I31
[root@rocky9 /]# top -bn1 | grep "Cpu(s)"
%Cpu(s):  0.0 us,  0.0 sy,  0.0 ni,100.0 id,  0.0 wa,  0.0 hi,  0.0 si,  0.0 st
[root@rocky9 /]# free -m
               total        used        free      shared  buff/cache   available
Mem:            8192         474        4643           8        3081        7717
Swap:           8191           0        8191
[root@rocky9 /]# 
[root@rocky9 /]# 