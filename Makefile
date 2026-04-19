# One command to build a .deb (requires: cargo, dpkg-deb).
.PHONY: package deb
package deb:
	@./scripts/build-deb.sh
