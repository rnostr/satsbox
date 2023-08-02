case "$1" in
memory)  
  echo "use sqlite memory database for test"
  unset RUST_TEST_THREADS
  export SATSBOX_DB_URL=sqlite::memory:
  ;;

sqlite)  
  echo "use sqlite file database for test"
  touch satsbox.sqlite
  export RUST_TEST_THREADS=1
  export SATSBOX_DB_URL=sqlite://satsbox.sqlite
  ;;

postgres)  
  echo "use postgres database for test"
  export RUST_TEST_THREADS=1
  export SATSBOX_DB_URL=postgres://test:test@127.0.0.1:8432/satsbox
  ;;

mariadb)  
  echo "use mariadb database for test"
  export RUST_TEST_THREADS=1
  export SATSBOX_DB_URL=mysql://test:test@127.0.0.1:8306/satsbox
  ;;

*)      
  echo "Usage: env.sh {memory|sqlite|postgres|mariadb}"
  ;;
esac
