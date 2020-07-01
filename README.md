# ipmi-fans

I have a refurbished Supermicro SuperServer 6018U-TR4T+ in my basement. Unfortunately, this kind of server hardware is not optimized for noise, and when the fans are running at 100% I can hear the server through the floor.

I was running a modified version of [hybrid_fan_controller.pl], but on a warm day with people on the Minecraft server the fans would pulse on and off as the temperature bounced between the medium and high zones.

This program has a fan curve so the speed smoothly increases and inertia to prevent the fan speed from constantly adjusting up and down within a 12% range. It uses Rust instead of Perl, and libfreeipmi instead of ipmitool.

The functionality to set the fan speed may work on SuperServer's in general, but this program makes some assumptions about the number and configuration of fans and how fan speed is read back from the BMC via IPMI, so it may run incorrectly on models besides the 6018U-TR4T+, and may even run incorrectly on different 6018U-TR4T+ systems or different IPMI software revisions on the same 6018U-TR4T+.

I am assuming that in the event of a malfunction leading to overheating, the BMC will force the fans to full, the CPUs will throttle, and the system will power off before seriously damaging itself.

Use at your own risk.

[hybrid_fan_controller.pl]: https://www.ixsystems.com/community/threads/script-hybrid-cpu-hd-fan-zone-controller.46159/
