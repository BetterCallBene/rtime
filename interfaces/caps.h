#define CAPABILITY_FUNCTION_NAME_LEN        256
#define CAPABILITY_NUMBER_OF_CAPABILITIES   20

typedef struct capability
{
   char name[CAPABILITY_FUNCTION_NAME_LEN]; // name of the capability
   void* function; // function pointer
} Capability;


typedef struct capabilities_
{
    Capability capability[CAPABILITY_NUMBER_OF_CAPABILITIES]; // array of capabilities
    int n_capabilities; // number of capabilities
} Capabilities;