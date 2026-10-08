# Native open component build diagnosis

Initial02/03 fails because assigning the SourceRows callback closure to a local
variable fixes its inferred lifetime too early for the higher-ranked FnOnce port.
Pass the narrow delegate closure inline at each observe entrypoint; both still
call the same decide_on semantic implementation. No owner or lifetime contract
is relaxed and no runtime test ran from the failed build.
